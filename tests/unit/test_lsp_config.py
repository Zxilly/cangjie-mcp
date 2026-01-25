"""Tests for LSP configuration module."""

from __future__ import annotations

import sys
from pathlib import Path

import pytest

from cangjie_mcp.lsp.config import (
    LSPSettings,
    _get_std_lib_path,
    _get_target_dir,
    _get_target_lib,
    build_init_options,
    get_resolver_require_path,
)


class TestLSPSettings:
    """Tests for LSPSettings dataclass."""

    def test_default_values(self, tmp_path: Path) -> None:
        settings = LSPSettings(
            sdk_path=tmp_path / "sdk",
            workspace_path=tmp_path / "workspace",
        )
        assert settings.log_enabled is False
        assert settings.log_path is None
        assert settings.init_timeout == 45000
        assert settings.disable_auto_import is True

    def test_get_lsp_args_default(self, tmp_path: Path) -> None:
        settings = LSPSettings(
            sdk_path=tmp_path / "sdk",
            workspace_path=tmp_path / "workspace",
        )
        args = settings.get_lsp_args()
        assert "src" in args
        assert "--disableAutoImport" in args
        assert "--enable-log=false" in args

    def test_get_lsp_args_with_logging(self, tmp_path: Path) -> None:
        settings = LSPSettings(
            sdk_path=tmp_path / "sdk",
            workspace_path=tmp_path / "workspace",
            log_enabled=True,
            log_path=tmp_path / "logs",
        )
        args = settings.get_lsp_args()
        assert "-V" in args
        assert "--enable-log=true" in args
        assert f"--log-path={tmp_path / 'logs'}" in args

    def test_lsp_server_path(self, tmp_path: Path) -> None:
        settings = LSPSettings(
            sdk_path=tmp_path / "sdk",
            workspace_path=tmp_path / "workspace",
        )
        path = settings.lsp_server_path
        assert "tools" in str(path)
        assert "bin" in str(path)
        if sys.platform == "win32":
            assert path.name == "LSPServer.exe"
        else:
            assert path.name == "LSPServer"


class TestBuildInitOptions:
    """Tests for build_init_options function."""

    @pytest.fixture
    def workspace(self, tmp_path: Path) -> Path:
        """Create a workspace with cjpm.toml."""
        workspace = tmp_path / "workspace"
        workspace.mkdir()

        cjpm_toml = """
[package]
name = "test_project"

[dependencies]
my_dep = { path = "./dep" }
"""
        (workspace / "cjpm.toml").write_text(cjpm_toml)
        (workspace / "dep").mkdir()

        return workspace

    @pytest.fixture
    def sdk(self, tmp_path: Path) -> Path:
        """Create a fake SDK directory."""
        sdk = tmp_path / "sdk"
        sdk.mkdir()
        (sdk / "lib" / "src").mkdir(parents=True)
        (sdk / "tools" / "bin").mkdir(parents=True)
        return sdk

    def test_basic_options(self, workspace: Path, sdk: Path) -> None:
        settings = LSPSettings(sdk_path=sdk, workspace_path=workspace)
        options = build_init_options(settings)

        assert "multiModuleOption" in options
        assert "modulesHomeOption" in options
        assert "stdLibPathOption" in options
        assert "targetLib" in options
        assert "telemetryOption" in options
        assert "conditionCompileOption" in options

    def test_multi_module_option_populated(self, workspace: Path, sdk: Path) -> None:
        settings = LSPSettings(sdk_path=sdk, workspace_path=workspace)
        options = build_init_options(settings)

        multi_module = options["multiModuleOption"]
        assert len(multi_module) > 0

        # Find main module
        main_module = None
        for _uri, module in multi_module.items():
            if module.get("name") == "test_project":
                main_module = module
                break

        assert main_module is not None
        assert "my_dep" in main_module.get("requires", {})

    def test_without_cjpm_toml(self, sdk: Path, tmp_path: Path) -> None:
        empty_workspace = tmp_path / "empty"
        empty_workspace.mkdir()

        settings = LSPSettings(sdk_path=sdk, workspace_path=empty_workspace)
        options = build_init_options(settings)

        # Should return empty multiModuleOption
        assert options["multiModuleOption"] == {}


class TestGetTargetLib:
    """Tests for _get_target_lib function."""

    @pytest.mark.skipif(sys.platform != "win32", reason="Windows-specific test")
    def test_windows_uses_cache_lsp(self, tmp_path: Path) -> None:
        result = _get_target_lib(tmp_path)
        assert ".cache" in result
        assert "lsp" in result

    @pytest.mark.skipif(sys.platform == "win32", reason="Unix-specific test")
    def test_unix_uses_target_release(self, tmp_path: Path) -> None:
        target_dir = tmp_path / "target"
        target_dir.mkdir()
        (target_dir / "release").mkdir()

        result = _get_target_lib(tmp_path)
        assert "release" in result

    @pytest.mark.skipif(sys.platform == "win32", reason="Unix-specific test")
    def test_unix_falls_back_to_debug(self, tmp_path: Path) -> None:
        result = _get_target_lib(tmp_path)
        assert "debug" in result


class TestGetTargetDir:
    """Tests for _get_target_dir function."""

    def test_default_target_dir(self, tmp_path: Path) -> None:
        result = _get_target_dir(tmp_path)
        assert result == tmp_path / "target"

    def test_custom_target_dir(self, tmp_path: Path) -> None:
        cjpm_toml = """
[package]
name = "test"
target-dir = "build"
"""
        (tmp_path / "cjpm.toml").write_text(cjpm_toml)

        result = _get_target_dir(tmp_path)
        assert result.name == "build"

    def test_no_cjpm_toml(self, tmp_path: Path) -> None:
        result = _get_target_dir(tmp_path)
        assert result == tmp_path / "target"


class TestGetStdLibPath:
    """Tests for _get_std_lib_path function."""

    def test_uses_sdk_lib_src(self, tmp_path: Path) -> None:
        sdk = tmp_path / "sdk"
        (sdk / "lib" / "src").mkdir(parents=True)

        result = _get_std_lib_path(sdk, "")
        assert "lib" in result
        assert "src" in result

    def test_falls_back_to_sdk_path(self, tmp_path: Path) -> None:
        sdk = tmp_path / "sdk"
        sdk.mkdir()

        result = _get_std_lib_path(sdk, "")
        # Returns the path even if it doesn't exist
        assert "lib" in result
        assert "src" in result


class TestGetResolverRequirePath:
    """Tests for get_resolver_require_path function."""

    def test_empty_by_default(self, tmp_path: Path) -> None:
        workspace = tmp_path / "workspace"
        workspace.mkdir()

        cjpm_toml = """
[package]
name = "test"
"""
        (workspace / "cjpm.toml").write_text(cjpm_toml)

        sdk = tmp_path / "sdk"
        sdk.mkdir()

        settings = LSPSettings(sdk_path=sdk, workspace_path=workspace)
        build_init_options(settings)

        require_path = get_resolver_require_path()
        assert require_path == ""

    def test_contains_c_ffi_paths(self, tmp_path: Path) -> None:
        workspace = tmp_path / "workspace"
        workspace.mkdir()
        (workspace / "native").mkdir()

        cjpm_toml = """
[package]
name = "test"

[ffi.c]
my_lib = { path = "./native" }
"""
        (workspace / "cjpm.toml").write_text(cjpm_toml)

        sdk = tmp_path / "sdk"
        sdk.mkdir()

        settings = LSPSettings(sdk_path=sdk, workspace_path=workspace)
        build_init_options(settings)

        require_path = get_resolver_require_path()
        assert "native" in require_path
