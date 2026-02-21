"""Tests for LSP utility functions."""

from __future__ import annotations

import os
import sys
from pathlib import Path
from unittest import mock

import pytest

from cangjie_mcp.lsp.utils import (
    get_cjpm_config_path,
    get_path_separator,
    get_real_path,
    load_toml_safe,
    merge_unique_strings,
    normalize_path,
    path_to_uri,
    strip_trailing_separator,
    uri_to_path,
)


class TestGetRealPath:
    """Tests for get_real_path function."""

    def test_no_substitution_needed(self) -> None:
        assert get_real_path("/path/to/file") == "/path/to/file"

    def test_normalizes_backslashes(self) -> None:
        assert get_real_path("C:\\path\\to\\file") == "C:/path/to/file"

    def test_substitutes_env_var(self) -> None:
        with mock.patch.dict(os.environ, {"MY_VAR": "/my/path"}):
            assert get_real_path("${MY_VAR}/subdir") == "/my/path/subdir"

    def test_missing_env_var_unchanged(self) -> None:
        result = get_real_path("${NONEXISTENT_VAR_12345}/subdir")
        assert "${NONEXISTENT_VAR_12345}" in result

    def test_invalid_input(self) -> None:
        assert get_real_path("") == ""
        assert get_real_path(None) is None  # type: ignore


class TestPathToUri:
    """Tests for path_to_uri function."""

    def test_path_object(self) -> None:
        result = path_to_uri(Path("/test/path"))
        assert result.startswith("file://")

    def test_string_path(self) -> None:
        result = path_to_uri("/test/path")
        assert result.startswith("file://")

    @pytest.mark.skipif(sys.platform != "win32", reason="Windows-specific test")
    def test_windows_path(self) -> None:
        result = path_to_uri("C:\\test\\path")
        assert result == "file:///C:/test/path"

    @pytest.mark.skipif(sys.platform == "win32", reason="Unix-specific test")
    def test_unix_path(self) -> None:
        result = path_to_uri("/test/path")
        assert result == "file:///test/path"


class TestUriToPath:
    """Tests for uri_to_path function."""

    def test_basic_uri(self) -> None:
        uri = "file:///test/path"
        result = uri_to_path(uri)
        assert isinstance(result, Path)

    def test_non_uri_passthrough(self) -> None:
        path = "/test/path"
        result = uri_to_path(path)
        assert result == Path(path)


class TestGetPathSeparator:
    """Tests for get_path_separator function."""

    @pytest.mark.skipif(sys.platform != "win32", reason="Windows-specific test")
    def test_windows_separator(self) -> None:
        assert get_path_separator() == ";"

    @pytest.mark.skipif(sys.platform == "win32", reason="Unix-specific test")
    def test_unix_separator(self) -> None:
        assert get_path_separator() == ":"


class TestMergeUniqueStrings:
    """Tests for merge_unique_strings function."""

    def test_empty_arrays(self) -> None:
        assert merge_unique_strings() == []

    def test_single_array(self) -> None:
        assert merge_unique_strings(["a", "b", "c"]) == ["a", "b", "c"]

    def test_merge_with_duplicates(self) -> None:
        result = merge_unique_strings(["a", "b"], ["b", "c"])
        assert result == ["a", "b", "c"]

    def test_preserves_order(self) -> None:
        result = merge_unique_strings(["c", "a"], ["b", "a"])
        assert result == ["c", "a", "b"]


class TestLoadTomlSafe:
    """Tests for load_toml_safe function."""

    def test_nonexistent_file(self, tmp_path: Path) -> None:
        result = load_toml_safe(tmp_path / "nonexistent.toml")
        assert result == {}

    def test_valid_toml(self, tmp_path: Path) -> None:
        toml_file = tmp_path / "test.toml"
        toml_file.write_text('[package]\nname = "test"\n')
        result = load_toml_safe(toml_file)
        assert result == {"package": {"name": "test"}}

    def test_invalid_toml(self, tmp_path: Path) -> None:
        toml_file = tmp_path / "invalid.toml"
        toml_file.write_text("this is not valid toml [[[")
        result = load_toml_safe(toml_file)
        assert result == {}


class TestStripTrailingSeparator:
    """Tests for strip_trailing_separator function."""

    def test_no_trailing_separator(self) -> None:
        assert strip_trailing_separator("/path/to/dir") == "/path/to/dir"

    def test_forward_slash(self) -> None:
        assert strip_trailing_separator("/path/to/dir/") == "/path/to/dir"

    def test_backslash(self) -> None:
        assert strip_trailing_separator("C:\\path\\to\\dir\\") == "C:\\path\\to\\dir"

    def test_empty_string(self) -> None:
        assert strip_trailing_separator("") == ""


class TestNormalizePath:
    """Tests for normalize_path function."""

    def test_absolute_path_unchanged(self) -> None:
        if sys.platform == "win32":
            base = Path("C:/base")
            result = normalize_path("C:/absolute/path", base)
            assert result == Path("C:/absolute/path")
        else:
            base = Path("/base")
            result = normalize_path("/absolute/path", base)
            assert result == Path("/absolute/path")

    def test_relative_path_resolved(self) -> None:
        base = Path.cwd()
        result = normalize_path("relative/path", base)
        assert result.is_absolute()
        assert str(result).endswith("relative/path".replace("/", os.sep))


class TestGetCjpmConfigPath:
    """Tests for get_cjpm_config_path function."""

    def test_uses_cjpm_config_env(self) -> None:
        with mock.patch.dict(os.environ, {"CJPM_CONFIG": "/custom/cjpm"}):
            result = get_cjpm_config_path("git")
            assert result == Path("/custom/cjpm/git")

    def test_falls_back_to_home(self) -> None:
        env = os.environ.copy()
        env.pop("CJPM_CONFIG", None)
        with mock.patch.dict(os.environ, env, clear=True):
            result = get_cjpm_config_path("repository")
            assert ".cjpm" in str(result)
            assert "repository" in str(result)
