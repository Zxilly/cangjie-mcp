"""Common utilities and helper functions."""

from __future__ import annotations

from collections.abc import Callable
from pathlib import Path
from threading import Lock
from typing import TYPE_CHECKING

from rich.console import Console
from rich.progress import BarColumn, Progress, SpinnerColumn, TextColumn

if TYPE_CHECKING:
    from cangjie_mcp.config import Settings

console = Console()

# Default encoding for file operations
ENCODING = "utf-8"


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


def create_download_progress() -> Progress:
    """Create a Rich progress bar for download operations.

    Returns:
        Configured Progress instance
    """
    return Progress(
        SpinnerColumn(),
        TextColumn("[progress.description]{task.description}"),
        BarColumn(),
        TextColumn("[progress.percentage]{task.percentage:>3.0f}%"),
        console=console,
    )


def ensure_dir(path: Path) -> Path:
    """Ensure directory exists, creating if necessary.

    Args:
        path: Directory path to ensure exists

    Returns:
        The same path (for chaining)
    """
    path.mkdir(parents=True, exist_ok=True)
    return path


def extract_str_metadata(metadata: dict[str, str], *keys: str) -> dict[str, str]:
    """Extract string values from metadata dictionary.

    Args:
        metadata: Source metadata dictionary
        *keys: Keys to extract

    Returns:
        Dictionary with extracted string values (empty string for missing keys)
    """
    return {k: str(metadata.get(k, "")) for k in keys}


# Import here to avoid circular imports in type hints
from pydantic import BaseModel  # noqa: E402


class JsonFileModel(BaseModel):
    """Base model with JSON file I/O utilities.

    Provides convenient methods for saving to and loading from JSON files.

    Example:
        >>> class MyConfig(JsonFileModel):
        ...     name: str
        ...     value: int
        >>> config = MyConfig(name="test", value=42)
        >>> config.save_to_file(Path("config.json"))
        >>> loaded = MyConfig.load_from_file(Path("config.json"))
    """

    def save_to_file(self, path: Path, indent: int = 2) -> None:
        """Save model to JSON file.

        Args:
            path: File path to write to
            indent: JSON indentation level
        """
        path.write_text(self.model_dump_json(indent=indent), encoding=ENCODING)

    @classmethod
    def load_from_file(cls, path: Path) -> JsonFileModel:
        """Load model from JSON file.

        Args:
            path: File path to read from

        Returns:
            Model instance loaded from file

        Raises:
            FileNotFoundError: If file does not exist
            ValidationError: If JSON is invalid
        """
        return cls.model_validate_json(path.read_text(encoding=ENCODING))
