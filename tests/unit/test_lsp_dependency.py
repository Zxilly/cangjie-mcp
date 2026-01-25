"""Tests for LSP dependency resolver."""

from __future__ import annotations

from pathlib import Path

import pytest

from cangjie_mcp.lsp.dependency import (
    Dependency,
    DependencyResolver,
    ModuleOption,
    PackageRequires,
)


class TestDependency:
    """Tests for Dependency dataclass."""

    def test_create(self) -> None:
        dep = Dependency(path="file:///test/path")
        assert dep.path == "file:///test/path"


class TestPackageRequires:
    """Tests for PackageRequires dataclass."""

    def test_defaults(self) -> None:
        req = PackageRequires()
        assert req.package_option == {}
        assert req.path_option == []

    def test_with_values(self) -> None:
        req = PackageRequires(
            package_option={"lib": "file:///lib"},
            path_option=["file:///path1"],
        )
        assert req.package_option["lib"] == "file:///lib"
        assert len(req.path_option) == 1


class TestModuleOption:
    """Tests for ModuleOption dataclass."""

    def test_defaults(self) -> None:
        opt = ModuleOption()
        assert opt.name == ""
        assert opt.requires == {}
        assert opt.package_requires is None
        assert opt.java_requires is None

    def test_to_dict_minimal(self) -> None:
        opt = ModuleOption(name="test_module")
        result = opt.to_dict()
        assert result["name"] == "test_module"
        assert result["requires"] == {}
        assert "package_requires" not in result
        assert "java_requires" not in result

    def test_to_dict_full(self) -> None:
        opt = ModuleOption(
            name="test_module",
            requires={"dep1": Dependency(path="file:///dep1")},
            package_requires=PackageRequires(path_option=["file:///lib"]),
            java_requires=["java.module1"],
        )
        result = opt.to_dict()
        assert result["name"] == "test_module"
        assert result["requires"]["dep1"]["path"] == "file:///dep1"
        assert result["package_requires"]["path_option"] == ["file:///lib"]
        assert result["java_requires"] == ["java.module1"]


class TestDependencyResolver:
    """Tests for DependencyResolver class."""

    @pytest.fixture
    def workspace_dir(self, tmp_path: Path) -> Path:
        """Create a workspace directory."""
        return tmp_path / "workspace"

    @pytest.fixture
    def simple_workspace(self, workspace_dir: Path) -> Path:
        """Create a simple package workspace."""
        workspace_dir.mkdir(parents=True)

        cjpm_toml = """
[package]
name = "my_project"

[dependencies]
my_lib = { path = "./lib" }
"""
        (workspace_dir / "cjpm.toml").write_text(cjpm_toml)
        (workspace_dir / "lib").mkdir()

        return workspace_dir

    @pytest.fixture
    def workspace_mode(self, workspace_dir: Path) -> Path:
        """Create a workspace mode project."""
        workspace_dir.mkdir(parents=True)

        # Root cjpm.toml
        root_toml = """
[workspace]
members = ["./packages/core", "./packages/http"]

[dependencies]
shared = { path = "./shared" }
"""
        (workspace_dir / "cjpm.toml").write_text(root_toml)

        # Create member directories
        core_dir = workspace_dir / "packages" / "core"
        http_dir = workspace_dir / "packages" / "http"
        core_dir.mkdir(parents=True)
        http_dir.mkdir(parents=True)
        (workspace_dir / "shared").mkdir()

        # Core member
        core_toml = """
[package]
name = "core"
"""
        (core_dir / "cjpm.toml").write_text(core_toml)

        # HTTP member
        http_toml = """
[package]
name = "http"

[dependencies]
core = { path = "../core" }
"""
        (http_dir / "cjpm.toml").write_text(http_toml)

        return workspace_dir

    def test_resolve_simple_package(self, simple_workspace: Path) -> None:
        """Test resolving a simple package mode workspace."""
        resolver = DependencyResolver(workspace_path=simple_workspace)
        result = resolver.resolve()

        assert len(result) == 2  # main module + dependency

        # Find main module
        main_module = None
        for _uri, module in result.items():
            if module.get("name") == "my_project":
                main_module = module
                break

        assert main_module is not None
        assert "my_lib" in main_module["requires"]

    def test_resolve_workspace_mode(self, workspace_mode: Path) -> None:
        """Test resolving a workspace mode project."""
        resolver = DependencyResolver(workspace_path=workspace_mode)
        result = resolver.resolve()

        # Find members
        core_module = None
        http_module = None
        for _uri, module in result.items():
            if module.get("name") == "core":
                core_module = module
            elif module.get("name") == "http":
                http_module = module

        assert core_module is not None
        assert http_module is not None

        # Root dependencies inherited
        assert "shared" in core_module["requires"]
        assert "shared" in http_module["requires"]

        # Member-specific dependencies
        assert "core" in http_module["requires"]

    def test_resolve_nonexistent_workspace(self, tmp_path: Path) -> None:
        """Test resolving a workspace without cjpm.toml creates empty entry."""
        workspace = tmp_path / "empty"
        workspace.mkdir()

        resolver = DependencyResolver(workspace_path=workspace)
        result = resolver.resolve()

        # Creates an empty module entry for the workspace itself
        assert len(result) == 1
        module = next(iter(result.values()))
        assert module["name"] == ""
        assert module["requires"] == {}

    def test_get_require_path_empty(self, simple_workspace: Path) -> None:
        """Test require_path is empty when no FFI or bin-deps."""
        resolver = DependencyResolver(workspace_path=simple_workspace)
        resolver.resolve()

        assert resolver.get_require_path() == ""

    def test_java_requires_extraction(self, workspace_dir: Path) -> None:
        """Test Java FFI module extraction."""
        workspace_dir.mkdir(parents=True)

        cjpm_toml = """
[package]
name = "java_project"

[ffi.java]
java_module_1 = {}
java_module_2 = { extra = "config" }
"""
        (workspace_dir / "cjpm.toml").write_text(cjpm_toml)

        resolver = DependencyResolver(workspace_path=workspace_dir)
        result = resolver.resolve()

        main_module = None
        for _uri, module in result.items():
            if module.get("name") == "java_project":
                main_module = module
                break

        assert main_module is not None
        assert main_module.get("java_requires") == ["java_module_1", "java_module_2"]

    def test_c_ffi_adds_to_require_path(self, workspace_dir: Path) -> None:
        """Test C FFI adds paths to require_path."""
        workspace_dir.mkdir(parents=True)
        (workspace_dir / "native").mkdir()

        cjpm_toml = """
[package]
name = "c_project"

[ffi.c]
my_c_lib = { path = "./native" }
"""
        (workspace_dir / "cjpm.toml").write_text(cjpm_toml)

        resolver = DependencyResolver(workspace_path=workspace_dir)
        resolver.resolve()

        require_path = resolver.get_require_path()
        assert "native" in require_path

    def test_bin_dependencies_handling(self, workspace_dir: Path) -> None:
        """Test bin-dependencies handling."""
        workspace_dir.mkdir(parents=True)
        (workspace_dir / "lib").mkdir()

        cjpm_toml = """
[package]
name = "bin_dep_project"

[target.windows-x86_64.bin-dependencies]
path-option = ["./lib"]
"""
        (workspace_dir / "cjpm.toml").write_text(cjpm_toml)

        resolver = DependencyResolver(workspace_path=workspace_dir)
        result = resolver.resolve()

        main_module = None
        for _uri, module in result.items():
            if module.get("name") == "bin_dep_project":
                main_module = module
                break

        assert main_module is not None
        pkg_requires = main_module.get("package_requires", {})
        assert len(pkg_requires.get("path_option", [])) == 1

    def test_cycle_detection(self, workspace_dir: Path) -> None:
        """Test that cyclic dependencies don't cause infinite recursion."""
        workspace_dir.mkdir(parents=True)

        # Create A -> B -> A cycle
        a_dir = workspace_dir / "a"
        b_dir = workspace_dir / "b"
        a_dir.mkdir()
        b_dir.mkdir()

        a_toml = """
[package]
name = "a"

[dependencies]
b = { path = "../b" }
"""
        (a_dir / "cjpm.toml").write_text(a_toml)

        b_toml = """
[package]
name = "b"

[dependencies]
a = { path = "../a" }
"""
        (b_dir / "cjpm.toml").write_text(b_toml)

        root_toml = """
[package]
name = "root"

[dependencies]
a = { path = "./a" }
"""
        (workspace_dir / "cjpm.toml").write_text(root_toml)

        # Should not raise RecursionError
        resolver = DependencyResolver(workspace_path=workspace_dir)
        result = resolver.resolve()

        # All three should be present
        assert len(result) == 3
