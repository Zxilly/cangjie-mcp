"""Utility functions for LSP configuration.

This module provides path manipulation, URI conversion, and environment
variable substitution utilities used by the dependency resolver.
"""

from __future__ import annotations

import logging
import os
import re
import sys
import tomllib
from pathlib import Path
from typing import Any, TypeGuard
from urllib.parse import unquote

from pydantic import BaseModel, ConfigDict, Field, ValidationError

logger = logging.getLogger(__name__)

# File URI constants
FILE_URI_PREFIX = "file://"
FILE_URI_PREFIX_LEN = len(FILE_URI_PREFIX)

# CJPM configuration paths
CJPM_DEFAULT_DIR = ".cjpm"
CJPM_GIT_SUBDIR = "git"
CJPM_REPOSITORY_SUBDIR = "repository"


def is_dict(val: object) -> TypeGuard[dict[str, Any]]:
    """Type-narrowing check: isinstance(val, dict) with proper generic type."""
    return isinstance(val, dict)


def is_list(val: object) -> TypeGuard[list[Any]]:
    """Type-narrowing check: isinstance(val, list) with proper generic type."""
    return isinstance(val, list)


def get_real_path(path_str: str) -> str:
    """Substitute environment variables in path string.

    Replaces ${VAR_NAME} patterns with actual environment variable values.
    Also normalizes path separators to forward slashes.

    Args:
        path_str: Path string potentially containing ${VAR_NAME} patterns

    Returns:
        Path string with environment variables substituted
    """
    if not path_str:
        return path_str

    # Normalize to forward slashes
    path_str = path_str.replace("\\", "/")

    # Match ${VAR_NAME} pattern
    pattern = re.compile(r"\$\{(\w+)\}")

    def replace_var(match: re.Match[str]) -> str:
        var_name = match.group(1)
        env_value = os.environ.get(var_name, "")
        if var_name and env_value:
            return env_value.replace("\\", "/")
        return match.group(0)  # Return original if not found

    return pattern.sub(replace_var, path_str)


def path_to_uri(file_path: str | Path) -> str:
    """Convert a file system path to a file:// URI.

    Args:
        file_path: File system path (string or Path object)

    Returns:
        file:// URI string
    """
    if isinstance(file_path, Path):
        file_path = str(file_path)

    # Normalize path separators
    file_path = file_path.replace("\\", "/")

    # On Windows: C:/path/to/file -> file:///C:/path/to/file
    # On Unix: /path/to/file -> file:///path/to/file
    if sys.platform == "win32":
        # Windows paths need an extra slash
        return "file:///" + file_path
    else:
        return "file://" + file_path


def uri_to_path(uri: str) -> Path:
    """Convert a file:// URI back to a Path.

    Args:
        uri: file:// URI string

    Returns:
        Path object
    """
    if not uri.startswith(FILE_URI_PREFIX):
        return Path(uri)

    # Remove file:// prefix
    path_str = uri[FILE_URI_PREFIX_LEN:]

    # On Windows, remove extra leading slash if present
    if sys.platform == "win32" and path_str.startswith("/"):
        path_str = path_str[1:]

    # URL decode the path
    path_str = unquote(path_str)

    return Path(path_str)


def get_cjpm_config_path(subdir: str) -> Path:
    """Get path to a CJPM configuration subdirectory.

    Uses CJPM_CONFIG environment variable if set, otherwise uses ~/.cjpm.

    Args:
        subdir: Subdirectory name (e.g., 'git', 'repository')

    Returns:
        Path to the configuration subdirectory
    """
    # Check for CJPM_CONFIG environment variable
    cjpm_config = os.environ.get("CJPM_CONFIG")
    if cjpm_config:
        return Path(cjpm_config) / subdir

    # Use home directory
    home_dir = os.environ.get("USERPROFILE", "") if sys.platform == "win32" else os.environ.get("HOME", "")

    if not home_dir:
        home_dir = str(Path.home())

    return Path(home_dir) / CJPM_DEFAULT_DIR / subdir


def normalize_path(path_str: str, base_path: Path) -> Path:
    """Normalize a path string relative to a base path.

    Handles environment variable substitution and relative path resolution.

    Args:
        path_str: Path string (potentially relative)
        base_path: Base path for resolving relative paths

    Returns:
        Normalized absolute Path
    """
    # Substitute environment variables
    path_str = get_real_path(path_str)

    # Normalize the path
    path = Path(path_str)

    # Resolve relative paths against base_path
    if not path.is_absolute():
        path = base_path / path

    # Normalize (resolve . and ..)
    return path.resolve()


def get_path_separator() -> str:
    """Get the platform-specific PATH separator.

    Returns:
        ';' on Windows, ':' on Unix-like systems
    """
    return ";" if sys.platform == "win32" else ":"


def merge_unique_strings(*arrays: list[str]) -> list[str]:
    """Merge multiple string lists and remove duplicates.

    Args:
        *arrays: Variable number of string lists

    Returns:
        Merged list with unique items
    """
    seen: set[str] = set()
    result: list[str] = []

    for arr in arrays:
        for item in arr:
            if item not in seen:
                seen.add(item)
                result.append(item)

    return result


def load_toml_safe(toml_path: Path) -> dict[str, Any]:
    """Safely load and parse a TOML file.

    Args:
        toml_path: Path to the TOML file

    Returns:
        Parsed TOML as dictionary, or empty dict on error
    """
    if not toml_path.exists():
        return {}

    try:
        with toml_path.open("rb") as f:
            return tomllib.load(f)
    except tomllib.TOMLDecodeError as e:
        logger.warning(f"Failed to parse {toml_path}: {e}")
        return {}


# ====================================
# TOML Schema Models (Pydantic)
# ====================================


class CjpmPackage(BaseModel):
    """[package] section of cjpm.toml."""

    model_config = ConfigDict(extra="allow", populate_by_name=True)

    name: str = ""
    target_dir: str = Field(default="", alias="target-dir")


class CjpmWorkspace(BaseModel):
    """[workspace] section of cjpm.toml."""

    model_config = ConfigDict(extra="allow")

    members: list[str] = Field(default_factory=list)


class CjpmDepConfig(BaseModel):
    """Dependency table value (path or git dependency)."""

    model_config = ConfigDict(extra="allow")

    path: str | None = None
    git: str | None = None


class CjpmBinDependencies(BaseModel):
    """bin-dependencies section within a target."""

    model_config = ConfigDict(extra="allow", populate_by_name=True)

    path_option: list[str] = Field(default_factory=list, alias="path-option")
    package_option: dict[str, str] = Field(default_factory=dict, alias="package-option")


class CjpmTargetConfig(BaseModel):
    """A single target platform configuration."""

    model_config = ConfigDict(extra="allow", populate_by_name=True)

    dependencies: dict[str, str | CjpmDepConfig] = Field(default_factory=dict)
    dev_dependencies: dict[str, str | CjpmDepConfig] = Field(default_factory=dict, alias="dev-dependencies")
    bin_dependencies: CjpmBinDependencies | None = Field(default=None, alias="bin-dependencies")


class CjpmCModule(BaseModel):
    """A C FFI module configuration."""

    model_config = ConfigDict(extra="allow")

    path: str = ""


class CjpmFfi(BaseModel):
    """[ffi] section of cjpm.toml."""

    model_config = ConfigDict(extra="allow")

    java: dict[str, Any] = Field(default_factory=dict)
    c: dict[str, CjpmCModule] = Field(default_factory=dict)


class CjpmToml(BaseModel):
    """Complete cjpm.toml structure."""

    model_config = ConfigDict(extra="allow", populate_by_name=True)

    package: CjpmPackage | None = None
    workspace: CjpmWorkspace | None = None
    dependencies: dict[str, str | CjpmDepConfig] = Field(default_factory=dict)
    dev_dependencies: dict[str, str | CjpmDepConfig] = Field(default_factory=dict, alias="dev-dependencies")
    target: dict[str, CjpmTargetConfig] = Field(default_factory=dict)
    ffi: CjpmFfi | None = None


class CjpmLockRequire(BaseModel):
    """A single entry in cjpm.lock requires section."""

    model_config = ConfigDict(extra="allow")

    commitId: str = ""


class CjpmLock(BaseModel):
    """cjpm.lock structure."""

    model_config = ConfigDict(extra="allow")

    requires: dict[str, CjpmLockRequire] = Field(default_factory=dict)


def load_cjpm_toml(toml_path: Path) -> CjpmToml | None:
    """Load and validate a cjpm.toml file as a typed model.

    Args:
        toml_path: Path to the cjpm.toml file

    Returns:
        Validated CjpmToml model, or None if file is missing/empty/invalid
    """
    raw = load_toml_safe(toml_path)
    if not raw:
        return None
    try:
        return CjpmToml.model_validate(raw)
    except ValidationError as e:
        logger.warning(f"Invalid cjpm.toml at {toml_path}: {e}")
        return None


def load_cjpm_lock(lock_path: Path) -> CjpmLock | None:
    """Load and validate a cjpm.lock file as a typed model.

    Args:
        lock_path: Path to the cjpm.lock file

    Returns:
        Validated CjpmLock model, or None if file is missing/empty/invalid
    """
    raw = load_toml_safe(lock_path)
    if not raw:
        return None
    try:
        return CjpmLock.model_validate(raw)
    except ValidationError as e:
        logger.warning(f"Invalid cjpm.lock at {lock_path}: {e}")
        return None


def strip_trailing_separator(path_str: str) -> str:
    """Strip trailing path separator from a path string.

    Args:
        path_str: Path string to clean

    Returns:
        Path string without trailing separator
    """
    if path_str.endswith(("/", "\\")):
        return path_str[:-1]
    return path_str
