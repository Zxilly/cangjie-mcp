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
from typing import Any
from urllib.parse import unquote

logger = logging.getLogger(__name__)

# File URI constants
FILE_URI_PREFIX = "file://"
FILE_URI_PREFIX_LEN = len(FILE_URI_PREFIX)

# CJPM configuration paths
CJPM_DEFAULT_DIR = ".cjpm"
CJPM_GIT_SUBDIR = "git"
CJPM_REPOSITORY_SUBDIR = "repository"


def check_is_valid(value: object) -> bool:
    """Check if a value is valid (non-empty, non-null).

    Args:
        value: Value to check

    Returns:
        True if value is valid, False otherwise
    """
    if value is None:
        return False
    if isinstance(value, str) and value == "":
        return False
    if isinstance(value, bool):
        return value
    return not (isinstance(value, int | float) and value == 0)


def get_real_path(path_str: str) -> str:
    """Substitute environment variables in path string.

    Replaces ${VAR_NAME} patterns with actual environment variable values.
    Also normalizes path separators to forward slashes.

    Args:
        path_str: Path string potentially containing ${VAR_NAME} patterns

    Returns:
        Path string with environment variables substituted
    """
    if not check_is_valid(path_str):
        return path_str

    # Normalize to forward slashes
    path_str = path_str.replace("\\", "/")

    # Match ${VAR_NAME} pattern
    pattern = re.compile(r"\$\{(\w+)\}")

    def replace_var(match: re.Match[str]) -> str:
        var_name = match.group(1)
        env_value = os.environ.get(var_name, "")
        if check_is_valid(var_name) and check_is_valid(env_value):
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
    if cjpm_config and check_is_valid(cjpm_config):
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
            # Defensive check for robustness with improperly typed input
            if item is not None and item not in seen:  # pyright: ignore[reportUnnecessaryComparison]
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
