"""Common utilities and helper functions."""

from __future__ import annotations

import logging
import sys
from collections.abc import Callable
from pathlib import Path
from threading import Lock
from typing import TYPE_CHECKING, Any, BinaryIO, TextIO

if TYPE_CHECKING:
    from cangjie_mcp.config import Settings

# Application logger
logger = logging.getLogger("cangjie_mcp")
_stdio_logger = logging.getLogger("cangjie_mcp.stdio")


class _BufferTee:
    """Wraps a binary I/O buffer to also log data via Python logging."""

    def __init__(self, original: BinaryIO, label: str) -> None:
        self._original = original
        self._label = label

    def write(self, data: bytes) -> int:
        result = self._original.write(data)
        self._log_data(data)
        return result

    def read(self, n: int = -1) -> bytes:
        data = self._original.read(n)
        self._log_data(data)
        return data

    def readline(self, limit: int = -1) -> bytes:
        data = self._original.readline(limit)
        self._log_data(data)
        return data

    def _log_data(self, data: bytes) -> None:
        if data:
            try:
                text = data.decode("utf-8", errors="replace")
                _stdio_logger.debug("[%s] %s", self._label, text.rstrip("\n"))
            except Exception:
                pass

    def __getattr__(self, name: str) -> Any:  # noqa: ANN401
        return getattr(self._original, name)


class _StreamWrapper:
    """Wraps a standard stream to intercept buffer access for stdio tee."""

    def __init__(self, original: TextIO, buffer_tee: _BufferTee) -> None:
        self._original = original
        self._buffer = buffer_tee

    @property
    def buffer(self) -> _BufferTee:
        return self._buffer

    def __getattr__(self, name: str) -> Any:  # noqa: ANN401
        return getattr(self._original, name)


def setup_logging(log_file: Path | None = None, debug: bool = False) -> None:
    """Configure application logging.

    Args:
        log_file: Path to log file. If None, no file logging is configured.
        debug: If True and log_file is set, also log stdio (MCP protocol) traffic.
    """
    if log_file is None:
        return

    # Ensure parent directory exists
    log_file.parent.mkdir(parents=True, exist_ok=True)

    # Configure Python logging to write to the file
    file_handler = logging.FileHandler(log_file, encoding="utf-8")
    file_handler.setFormatter(logging.Formatter("%(asctime)s [%(levelname)s] %(name)s: %(message)s"))

    root_logger = logging.getLogger()
    root_logger.addHandler(file_handler)
    root_logger.setLevel(logging.DEBUG if debug else logging.INFO)

    logger.info("Logging initialized - log_file=%s, debug=%s", log_file, debug)

    if debug:
        # Wrap stdin/stdout buffers to log MCP protocol traffic
        stdin_tee = _BufferTee(sys.stdin.buffer, "STDIN")
        stdout_tee = _BufferTee(sys.stdout.buffer, "STDOUT")
        sys.stdin = _StreamWrapper(sys.stdin, stdin_tee)
        sys.stdout = _StreamWrapper(sys.stdout, stdout_tee)
        logger.debug("Debug mode: stdio tee enabled")


def create_literal_validator(
    name: str,
    valid_values: tuple[str, ...],
) -> Callable[[str], str]:
    """Create a validator for Literal types.

    This factory function generates validator functions that check if a value
    is one of the allowed values and raise a typer.BadParameter if not.

    Args:
        name: Human-readable name for the parameter (used in error messages)
        valid_values: Tuple of valid string values

    Returns:
        A validator function that takes a string and returns it if valid

    Example:
        >>> _validate_lang = create_literal_validator("language", ("zh", "en"))
        >>> _validate_lang("zh")  # Returns "zh"
        >>> _validate_lang("fr")  # Raises typer.BadParameter
    """

    def validator(value: str) -> str:
        if value not in valid_values:
            import typer

            raise typer.BadParameter(f"Invalid {name}: {value}. Must be one of: {', '.join(valid_values)}.")
        return value

    return validator


def detect_device() -> str:
    """Detect the best available compute device for model inference.

    Checks for accelerators in order: NVIDIA CUDA, AMD ROCm,
    Intel XPU, Apple MPS, then falls back to CPU.

    Returns:
        Device string: "cuda", "xpu", "mps", or "cpu"
    """
    # NVIDIA CUDA / AMD ROCm (both exposed via torch.cuda)
    try:
        import torch

        if torch.cuda.is_available():
            device_name = torch.cuda.get_device_name(0)
            logger.info("Using CUDA device: %s", device_name)
            return "cuda"
    except ImportError:
        pass

    # Intel XPU (requires intel_extension_for_pytorch)
    try:
        import intel_extension_for_pytorch  # type: ignore[import-not-found]  # noqa: F401  # pyright: ignore[reportMissingImports, reportUnusedImport]
        import torch

        if hasattr(torch, "xpu") and torch.xpu.is_available():
            device_name = torch.xpu.get_device_name(0)
            logger.info("Using Intel XPU device: %s", device_name)
            return "xpu"
    except ImportError:
        pass

    # Apple MPS (Metal Performance Shaders)
    try:
        import torch

        if hasattr(torch.backends, "mps") and torch.backends.mps.is_available():
            logger.info("Using Apple MPS device")
            return "mps"
    except ImportError:
        pass

    logger.info("No GPU detected, using CPU")
    return "cpu"


# Cache the result so we only probe once per process
_detected_device: str | None = None


def get_device() -> str:
    """Get the cached detected device (probes hardware once per process).

    Returns:
        Device string: "cuda", "xpu", "mps", or "cpu"
    """
    global _detected_device
    if _detected_device is None:
        _detected_device = detect_device()
    return _detected_device


class SingletonProvider[T]:
    """Thread-safe singleton provider for lazy initialization.

    This class provides a thread-safe way to manage singleton instances
    with lazy initialization based on settings.

    Example:
        >>> def create_my_provider(settings: Settings) -> MyProvider:
        ...     return MyProvider(settings.some_option)
        >>> my_provider = SingletonProvider(create_my_provider)
        >>> instance = my_provider.get()  # Creates on first call
        >>> same_instance = my_provider.get()  # Returns cached
    """

    def __init__(self, create_fn: Callable[[Settings], T]) -> None:
        """Initialize singleton provider.

        Args:
            create_fn: Factory function that takes Settings and returns T
        """
        self._create_fn = create_fn
        self._instance: T | None = None
        self._lock = Lock()

    def get(self, settings: Settings | None = None) -> T:
        """Get or create the singleton instance (thread-safe).

        Args:
            settings: Optional settings to use for creation.
                     If None, uses global settings.

        Returns:
            The singleton instance
        """
        if self._instance is None:
            with self._lock:
                # Double-check locking pattern
                if self._instance is None:
                    if settings is None:
                        from cangjie_mcp.config import get_settings

                        settings = get_settings()
                    self._instance = self._create_fn(settings)
        return self._instance

    def reset(self) -> None:
        """Reset the singleton instance (useful for testing)."""
        with self._lock:
            self._instance = None

    @property
    def is_initialized(self) -> bool:
        """Check if the singleton has been initialized."""
        return self._instance is not None


def print_download_progress(downloaded: int, total: int) -> None:
    """Print download progress to stderr.

    Args:
        downloaded: Bytes downloaded so far
        total: Total bytes to download (0 if unknown)
    """
    if total > 0:
        pct = downloaded * 100 // total
        sys.stderr.write(f"\rDownloading... {pct}%")
    else:
        mb = downloaded / (1024 * 1024)
        sys.stderr.write(f"\rDownloading... {mb:.1f} MB")
    sys.stderr.flush()
