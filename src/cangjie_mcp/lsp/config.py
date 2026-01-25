"""LSP configuration and environment setup.

This module handles platform-specific environment configuration and
LSP initialization options for the Cangjie language server.
"""

from __future__ import annotations

import logging
import os
import platform
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import TYPE_CHECKING, Any, TypedDict

from cangjie_mcp.lsp.utils import load_toml_safe

if TYPE_CHECKING:
    from cangjie_mcp.lsp.dependency import DependencyResolver


class LSPInitOptions(TypedDict):
    """Type definition for LSP initialization options."""

    multiModuleOption: dict[str, Any]
    conditionCompileOption: dict[str, Any]
    singleConditionCompileOption: dict[str, Any]
    conditionCompilePaths: list[str]
    targetLib: str
    modulesHomeOption: str
    stdLibPathOption: str
    telemetryOption: bool
    extensionPath: str
    clangdFileStatus: bool
    fallbackFlags: list[str]


# Default timeout for LSP initialization (ms)
DEFAULT_LSP_INIT_TIMEOUT = 45000


@dataclass
class LSPSettings:
    """LSP-specific settings.

    Attributes:
        sdk_path: Path to Cangjie SDK (CANGJIE_HOME)
        workspace_path: Root path of the workspace
        log_enabled: Whether to enable LSP server logging
        log_path: Directory for LSP log files
        init_timeout: Initialization timeout in milliseconds
        disable_auto_import: Disable auto-import suggestions
    """

    sdk_path: Path
    workspace_path: Path
    log_enabled: bool = False
    log_path: Path | None = None
    init_timeout: int = DEFAULT_LSP_INIT_TIMEOUT
    disable_auto_import: bool = True

    def get_lsp_args(self) -> list[str]:
        """Build command-line arguments for LSP server."""
        args = ["src"]

        if self.disable_auto_import:
            args.append("--disableAutoImport")

        if self.log_enabled and self.log_path:
            args.extend(["-V", "--enable-log=true", f"--log-path={self.log_path}"])
        else:
            args.append("--enable-log=false")

        return args

    @property
    def lsp_server_path(self) -> Path:
        """Get the path to the LSP server executable."""
        exe_name = "LSPServer.exe" if sys.platform == "win32" else "LSPServer"
        return self.sdk_path / "tools" / "bin" / exe_name

    def validate(self) -> list[str]:
        """Validate settings and return list of errors."""
        errors: list[str] = []

        if not self.sdk_path.exists():
            errors.append(f"SDK path does not exist: {self.sdk_path}")

        if not self.lsp_server_path.exists():
            errors.append(f"LSP server not found: {self.lsp_server_path}")

        if not self.workspace_path.exists():
            errors.append(f"Workspace path does not exist: {self.workspace_path}")

        return errors


def get_platform_env(sdk_path: Path) -> dict[str, str]:
    """Get platform-specific environment variables for LSP server.

    Args:
        sdk_path: Path to Cangjie SDK

    Returns:
        Environment dictionary with required variables set
    """
    if sys.platform == "win32":
        return _get_windows_env(sdk_path)
    elif sys.platform == "darwin":
        return _get_darwin_env(sdk_path)
    else:
        return _get_linux_env(sdk_path)


def _get_linux_env(sdk_path: Path) -> dict[str, str]:
    """Get Linux environment variables."""
    try:
        result = subprocess.run(["arch"], capture_output=True, text=True, check=False)
        arch = result.stdout.strip() if result.returncode == 0 else "x86_64"
    except Exception:
        arch = "x86_64"

    env = os.environ.copy()
    ld_library_path = str(sdk_path / "lib" / f"linux_{arch}_llvm")
    bin_path = str(sdk_path / "tools" / "bin")

    existing_ld = env.get("LD_LIBRARY_PATH", "")
    existing_path = env.get("PATH", "")

    env["LD_LIBRARY_PATH"] = f"{ld_library_path}:{existing_ld}" if existing_ld else ld_library_path
    env["PATH"] = f"{bin_path}:{existing_path}" if existing_path else bin_path

    # Preserve CANGJIE_PATH if set
    if "CANGJIE_PATH" in os.environ:
        env["CANGJIE_PATH"] = os.environ["CANGJIE_PATH"]

    return env


def _get_darwin_env(sdk_path: Path) -> dict[str, str]:
    """Get macOS environment variables."""
    machine = platform.machine()
    arch = "aarch64" if machine == "arm64" else "x86_64"

    env = os.environ.copy()
    dyld_library_path = str(sdk_path / "lib" / f"darwin_{arch}_llvm")
    bin_path = str(sdk_path / "tools" / "bin")

    existing_dyld = env.get("DYLD_LIBRARY_PATH", "")
    existing_path = env.get("PATH", "")

    env["DYLD_LIBRARY_PATH"] = f"{dyld_library_path}:{existing_dyld}" if existing_dyld else dyld_library_path
    env["PATH"] = f"{bin_path}:{existing_path}" if existing_path else bin_path

    return env


def _get_windows_env(sdk_path: Path) -> dict[str, str]:
    """Get Windows environment variables."""
    env = os.environ.copy()

    paths = [
        str(sdk_path / "runtime" / "lib" / "windows_x86_64_llvm"),
        str(sdk_path / "bin"),
        str(sdk_path / "tools" / "bin"),
        env.get("PATH", ""),
    ]

    env["PATH"] = ";".join(p for p in paths if p)

    return env


logger = logging.getLogger(__name__)

# Global cache for last resolver instance
_last_resolver: DependencyResolver | None = None


def build_init_options(settings: LSPSettings) -> dict[str, Any]:
    """Build LSP initialization options.

    Uses DependencyResolver for complete dependency resolution including
    local path, Git, and version-based dependencies.

    Args:
        settings: LSP settings

    Returns:
        Initialization options dictionary for LSP server
    """
    global _last_resolver

    from cangjie_mcp.lsp.dependency import DependencyResolver

    sdk_str = str(settings.sdk_path)

    # Initialize resolver and resolve dependencies
    multi_module_option: dict[str, Any] = {}
    _last_resolver = None

    cjpm_path = settings.workspace_path / "cjpm.toml"
    if cjpm_path.exists():
        try:
            resolver = DependencyResolver(workspace_path=settings.workspace_path)
            multi_module_option = resolver.resolve()
            _last_resolver = resolver
        except Exception as e:
            logger.warning(f"Failed to resolve dependencies: {e}")
            multi_module_option = {}

    # Build initialization options (see LSPInitOptions TypedDict for structure)
    options: dict[str, Any] = {
        # Project options (from DependencyResolver)
        "multiModuleOption": multi_module_option,
        "conditionCompileOption": {},
        "singleConditionCompileOption": {},
        "conditionCompilePaths": [],
        "targetLib": _get_target_lib(settings.workspace_path),
        # Environment info (from setEnvInfo equivalent)
        "modulesHomeOption": sdk_str,
        "stdLibPathOption": _get_std_lib_path(settings.sdk_path, ""),
        "telemetryOption": False,
        "extensionPath": "",
        # Default options
        "clangdFileStatus": True,
        "fallbackFlags": [],
    }

    return options


def get_resolver_require_path() -> str:
    """Get the cached require_path from the last dependency resolution.

    This path contains directories needed for C FFI and bin-dependencies.
    It should be added to the PATH environment variable.

    Returns:
        Platform-specific path separator delimited string
    """
    return _last_resolver.get_require_path() if _last_resolver else ""


def _get_target_lib(workspace_path: Path) -> str:
    """Get the target library path for LSP.

    On Windows, uses .cache/lsp directory.
    On Linux/macOS, uses target/release or target/debug.

    Args:
        workspace_path: Workspace root path

    Returns:
        Path string for targetLib
    """
    if sys.platform == "win32":
        # Windows: use .cache/lsp
        return str(workspace_path / ".cache" / "lsp")
    else:
        # Linux/macOS: use target/release or target/debug
        target_dir = _get_target_dir(workspace_path)
        release_path = target_dir / "release"

        if release_path.exists():
            return str(release_path)

        return str(target_dir / "debug")


def _get_target_dir(workspace_path: Path) -> Path:
    """Get the target directory from cjpm.toml or default.

    Checks [package].target-dir in cjpm.toml, otherwise uses 'target'.

    Args:
        workspace_path: Workspace root path

    Returns:
        Path to target directory
    """
    default_target_dir = workspace_path / "target"
    cjpm_path = workspace_path / "cjpm.toml"

    config = load_toml_safe(cjpm_path)

    if not config or "package" not in config:
        return default_target_dir

    pkg = config["package"]
    if "target-dir" not in pkg:
        return default_target_dir

    custom_target_dir = pkg["target-dir"]
    if custom_target_dir and isinstance(custom_target_dir, str):
        return Path((workspace_path / custom_target_dir.strip()).resolve())

    return default_target_dir


def _get_std_lib_path(sdk_path: Path, extension_path: str) -> str:
    """Get the standard library path.

    Uses SDK lib/src if available, otherwise falls back to extension lib.

    Args:
        sdk_path: Path to SDK
        extension_path: Path to VSCode extension (unused in CLI context)

    Returns:
        Path string to standard library sources
    """
    sdk_std_lib = sdk_path / "lib" / "src"

    if sdk_std_lib.exists():
        return str(sdk_std_lib)

    # Fall back to extension lib if provided
    if extension_path:
        return str(Path(extension_path) / "lib")

    return str(sdk_std_lib)
