"""Dependency resolution for Cangjie LSP initialization.

This module provides complete dependency resolution from cjpm.toml files,
supporting local path, Git, and version-based dependencies with workspace
inheritance and cycle detection.
"""

from __future__ import annotations

import logging
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from cangjie_mcp.lsp.utils import (
    CJPM_GIT_SUBDIR,
    CJPM_REPOSITORY_SUBDIR,
    check_is_valid,
    get_cjpm_config_path,
    get_path_separator,
    get_real_path,
    load_toml_safe,
    merge_unique_strings,
    normalize_path,
    path_to_uri,
    strip_trailing_separator,
)

logger = logging.getLogger(__name__)

# TOML field names
CJPM_TOML = "cjpm.toml"
CJPM_LOCK = "cjpm.lock"
FIELD_PACKAGE = "package"
FIELD_NAME = "name"
FIELD_TARGET_DIR = "target-dir"
FIELD_WORKSPACE = "workspace"
FIELD_MEMBERS = "members"
FIELD_DEPENDENCIES = "dependencies"
FIELD_DEV_DEPENDENCIES = "dev-dependencies"
FIELD_TARGET = "target"
FIELD_BIN_DEPENDENCIES = "bin-dependencies"
FIELD_PATH_OPTION = "path-option"
FIELD_PACKAGE_OPTION = "package-option"
FIELD_FFI = "ffi"
FIELD_C = "c"
FIELD_JAVA = "java"

# Dependency type fields
DEP_PATH = "path"
DEP_GIT = "git"


@dataclass
class Dependency:
    """A resolved dependency with file:// URI path."""

    path: str  # file:// URI format


@dataclass
class PackageRequires:
    """Binary dependency configuration."""

    package_option: dict[str, str] = field(default_factory=dict)  # name -> file:// URI
    path_option: list[str] = field(default_factory=list)  # file:// URI list


@dataclass
class ModuleOption:
    """Module configuration for LSP initialization."""

    name: str = ""
    requires: dict[str, Dependency] = field(default_factory=dict)
    package_requires: PackageRequires | None = None
    java_requires: list[str] | None = None

    def to_dict(self) -> dict[str, Any]:
        """Convert to LSP initialization format."""
        result: dict[str, Any] = {
            "name": self.name,
            "requires": {k: {"path": v.path} for k, v in self.requires.items()},
        }
        if self.package_requires is not None:
            result["package_requires"] = {
                "package_option": self.package_requires.package_option,
                "path_option": self.package_requires.path_option,
            }
        if self.java_requires is not None:
            result["java_requires"] = self.java_requires
        return result


class DependencyResolver:
    """Resolves dependencies from cjpm.toml for LSP initialization.

    Supports:
    - Local path dependencies: { path = "./lib" }
    - Git dependencies: { git = "https://..." } (resolved via cjpm.lock)
    - Version dependencies: "1.0.0" (resolved via ~/.cjpm/repository)
    - Workspace mode with root dependency inheritance
    - Recursive parsing with cycle detection
    """

    def __init__(self, workspace_path: Path) -> None:
        """Initialize the dependency resolver.

        Args:
            workspace_path: Root path of the workspace
        """
        # Resolve to absolute path
        self.workspace_path = workspace_path.resolve()
        self.multi_module_option: dict[str, ModuleOption] = {}
        self.existed: list[str] = []  # Cycle detection
        self.root_module_lock_data: dict[str, Any] = {}
        self.require_path: str = ""  # Environment variable paths (for C FFI)

    def resolve(self) -> dict[str, dict[str, Any]]:
        """Resolve all dependencies and build multiModuleOption.

        Returns:
            multiModuleOption dictionary for LSP initialization
        """
        self._clear_state()
        self._get_multi_module_option()
        return {uri: opt.to_dict() for uri, opt in self.multi_module_option.items()}

    def get_require_path(self) -> str:
        """Get the accumulated require_path for environment variables.

        Returns:
            Semicolon or colon separated path string
        """
        return self.require_path

    def _clear_state(self) -> None:
        """Clear internal state for a fresh resolution."""
        self.multi_module_option = {}
        self.existed = []
        self.root_module_lock_data = {}
        self.require_path = ""

    def _get_multi_module_option(self) -> None:
        """Detect workspace vs package mode and process accordingly."""
        toml_path = self.workspace_path / CJPM_TOML
        toml_obj = load_toml_safe(toml_path)

        # Validate: workspace and package cannot coexist at root
        if FIELD_WORKSPACE in toml_obj and FIELD_PACKAGE in toml_obj:
            logger.warning("Both workspace and package fields found in cjpm.toml")
            return

        workspace = toml_obj.get(FIELD_WORKSPACE)

        if isinstance(workspace, dict) and check_is_valid(workspace) and FIELD_MEMBERS in workspace:
            self._process_workspace_mode(toml_obj, workspace)
        else:
            self._process_package_mode()

    def _process_workspace_mode(self, toml_obj: dict[str, Any], workspace: dict[str, Any]) -> None:
        """Process workspace mode with member inheritance.

        Args:
            toml_obj: Parsed root cjpm.toml
            workspace: Workspace configuration section
        """
        # 1. Parse root-level dependencies (inherited by all members)
        root_requires: dict[str, Dependency] = {}
        if FIELD_DEPENDENCIES in toml_obj:
            root_requires = self._get_requires(
                toml_obj[FIELD_DEPENDENCIES],
                self.workspace_path,
            )

        # 2. Parse root-level target configuration
        root_package_requires = PackageRequires()
        if FIELD_TARGET in toml_obj:
            root_package_requires = self._get_targets_package_requires(toml_obj[FIELD_TARGET], self.workspace_path)

        # 3. Process each member
        members = self._get_members(workspace, self.workspace_path)
        for member_path in members:
            self._find_all_toml(member_path, "")

            member_uri = path_to_uri(member_path)

            if member_uri not in self.multi_module_option:
                continue

            member_opt = self.multi_module_option[member_uri]

            # Merge root dependencies (root takes precedence for conflicts)
            merged_requires = {**member_opt.requires, **root_requires}
            member_opt.requires = merged_requires

            # Ensure package_requires exists
            if member_opt.package_requires is None:
                member_opt.package_requires = PackageRequires()

            # Merge root package_requires
            member_opt.package_requires.package_option = {
                **member_opt.package_requires.package_option,
                **root_package_requires.package_option,
            }
            member_opt.package_requires.path_option = merge_unique_strings(
                member_opt.package_requires.path_option,
                root_package_requires.path_option,
            )

    def _process_package_mode(self) -> None:
        """Process single package mode."""
        self._find_all_toml(self.workspace_path, "")

    def _get_members(self, workspace: dict[str, Any], base_path: Path) -> list[Path]:
        """Get valid member paths from workspace configuration.

        Args:
            workspace: Workspace configuration section
            base_path: Base path for resolving relative paths

        Returns:
            List of valid member paths
        """
        if not check_is_valid(workspace):
            return []

        members = workspace.get(FIELD_MEMBERS, [])
        if not isinstance(members, list):
            return []

        valid_paths: list[Path] = []
        invalid_paths: list[str] = []

        for member in members:
            if not isinstance(member, str):
                continue

            # Environment variable substitution
            member_str = get_real_path(member)
            member_path = normalize_path(member_str, base_path)

            if member_path.exists():
                valid_paths.append(member_path)
            else:
                invalid_paths.append(member)

        if invalid_paths:
            logger.warning(f"Members not found: {', '.join(invalid_paths)}")

        return valid_paths

    def _find_all_toml(self, module_path: Path, expected_name: str) -> None:
        """Recursively parse a module's cjpm.toml and its dependencies.

        Args:
            module_path: Path to the module directory
            expected_name: Expected package name (for validation)
        """
        module_uri = path_to_uri(module_path)

        # Cycle detection
        if module_uri in self.existed:
            return
        self.existed.append(module_uri)

        toml_path = module_path / CJPM_TOML
        module_option = ModuleOption()

        # If cjpm.toml doesn't exist, create empty entry
        if not toml_path.exists():
            self.multi_module_option[module_uri] = module_option
            return

        toml_obj = load_toml_safe(toml_path)

        # Validate TOML
        if not check_is_valid(toml_obj) or len(toml_obj) == 0:
            logger.warning(f"Invalid cjpm.toml in {module_uri}")
            self.multi_module_option[module_uri] = module_option
            return

        # Submodules cannot have workspace field
        if FIELD_WORKSPACE in toml_obj:
            logger.warning(f"workspace field not allowed in {toml_path}")
            self.multi_module_option[module_uri] = module_option
            return

        pkg = toml_obj.get(FIELD_PACKAGE, {})

        # Get module name
        if FIELD_NAME in pkg:
            pkg_name = pkg[FIELD_NAME]
            if expected_name and pkg_name != expected_name:
                logger.warning(f"Module name mismatch: expected {expected_name}, got {pkg_name}")
            module_option.name = pkg_name
        else:
            module_option.name = module_path.name

        # Parse dependencies
        self._find_dependencies(toml_obj, module_option, module_path)

        self.multi_module_option[module_uri] = module_option

    def _find_dependencies(
        self,
        toml_obj: dict[str, Any],
        module_option: ModuleOption,
        module_path: Path,
    ) -> None:
        """Parse all dependency sections from a cjpm.toml.

        Args:
            toml_obj: Parsed TOML configuration
            module_option: ModuleOption to populate
            module_path: Path to the module directory
        """
        # 1. Parse [target.*.bin-dependencies]
        if FIELD_TARGET in toml_obj:
            if module_option.package_requires is None:
                module_option.package_requires = PackageRequires()

            target_pkg_reqs = self._get_targets_package_requires(toml_obj[FIELD_TARGET], module_path)

            module_option.package_requires.package_option = {
                **module_option.package_requires.package_option,
                **target_pkg_reqs.package_option,
            }
            module_option.package_requires.path_option = merge_unique_strings(
                module_option.package_requires.path_option,
                target_pkg_reqs.path_option,
            )

        # 2. Parse [ffi]
        if FIELD_FFI in toml_obj:
            ffi = toml_obj[FIELD_FFI]

            # Java FFI
            if FIELD_JAVA in ffi:
                module_option.java_requires = self._get_java_modules(ffi[FIELD_JAVA])

            # C FFI (only adds to environment, not in initOptions)
            if FIELD_C in ffi:
                self._process_c_modules(ffi[FIELD_C], module_path)

        # 3. Parse [dependencies]
        if FIELD_DEPENDENCIES in toml_obj:
            module_option.requires = self._get_requires(toml_obj[FIELD_DEPENDENCIES], module_path)

        # 4. Parse [dev-dependencies]
        if FIELD_DEV_DEPENDENCIES in toml_obj:
            dev_requires = self._get_requires(toml_obj[FIELD_DEV_DEPENDENCIES], module_path)
            module_option.requires = {**module_option.requires, **dev_requires}

        # 5. Parse [target.*.dependencies] and [target.*.dev-dependencies]
        if FIELD_TARGET in toml_obj:
            target_requires = self._get_targets_requires(toml_obj[FIELD_TARGET], module_path)
            module_option.requires = {**module_option.requires, **target_requires}

    def _get_targets_package_requires(self, target: dict[str, Any], base_path: Path) -> PackageRequires:
        """Parse bin-dependencies from all target sections.

        Args:
            target: Target configuration section
            base_path: Base path for resolving paths

        Returns:
            Aggregated PackageRequires
        """
        result = PackageRequires()

        for _target_name, target_config in target.items():
            if not isinstance(target_config, dict):
                continue

            if FIELD_BIN_DEPENDENCIES in target_config:
                pkg_reqs = self._get_package_requires(target_config[FIELD_BIN_DEPENDENCIES], base_path)
                result.package_option = {**result.package_option, **pkg_reqs.package_option}
                result.path_option = merge_unique_strings(result.path_option, pkg_reqs.path_option)

        return result

    def _get_package_requires(self, bin_deps: dict[str, Any], base_path: Path) -> PackageRequires:
        """Parse a single bin-dependencies section.

        Args:
            bin_deps: bin-dependencies configuration
            base_path: Base path for resolving paths

        Returns:
            PackageRequires object
        """
        result = PackageRequires()

        # Process path-option array
        if FIELD_PATH_OPTION in bin_deps:
            path_options = bin_deps[FIELD_PATH_OPTION]
            if isinstance(path_options, list):
                for p in path_options:
                    if not isinstance(p, str):
                        continue
                    lib_path = normalize_path(get_real_path(p), base_path)
                    lib_path_str = strip_trailing_separator(str(lib_path))

                    # Add to require_path
                    self._add_to_require_path(lib_path_str)

                    result.path_option.append(path_to_uri(lib_path_str))

        # Process package-option object
        if FIELD_PACKAGE_OPTION in bin_deps:
            pkg_options = bin_deps[FIELD_PACKAGE_OPTION]
            if isinstance(pkg_options, dict):
                for pkg_name, pkg_path in pkg_options.items():
                    if not isinstance(pkg_path, str):
                        continue
                    resolved_path = normalize_path(get_real_path(pkg_path), base_path)
                    resolved_path_str = str(resolved_path)

                    # Add parent directory to require_path
                    self._add_to_require_path(str(resolved_path.parent))

                    result.package_option[pkg_name] = path_to_uri(resolved_path_str)

        return result

    def _get_targets_requires(self, target: dict[str, Any], base_path: Path) -> dict[str, Dependency]:
        """Parse dependencies from all target sections.

        Args:
            target: Target configuration section
            base_path: Base path for resolving paths

        Returns:
            Aggregated dependencies
        """
        result: dict[str, Dependency] = {}

        for _target_name, target_config in target.items():
            if not isinstance(target_config, dict):
                continue

            # target.*.dependencies
            if FIELD_DEPENDENCIES in target_config:
                deps = self._get_requires(target_config[FIELD_DEPENDENCIES], base_path)
                result = {**result, **deps}

            # target.*.dev-dependencies
            if FIELD_DEV_DEPENDENCIES in target_config:
                deps = self._get_requires(target_config[FIELD_DEV_DEPENDENCIES], base_path)
                result = {**result, **deps}

        return result

    def _get_requires(self, dependencies: dict[str, Any], base_path: Path) -> dict[str, Dependency]:
        """Parse a dependencies section resolving all dependency types.

        Handles three types:
        - Local path: { path = "./lib" }
        - Git: { git = "url" } (resolved via cjpm.lock)
        - Version: "1.0.0" (resolved via ~/.cjpm/repository)

        Args:
            dependencies: Dependencies configuration
            base_path: Base path for resolving paths

        Returns:
            Dictionary of resolved dependencies
        """
        result: dict[str, Dependency] = {}

        for dep_name, dep in dependencies.items():
            if isinstance(dep, dict) and DEP_PATH in dep:
                # Type 1: Local path dependency
                dep_path_str = get_real_path(dep[DEP_PATH])
                dep_path = normalize_path(dep_path_str, base_path)

                # Check if dependency is a workspace
                if self._is_workspace(dep_path):
                    member_path = self._get_target_member_path(dep_name, dep_path)
                    if member_path:
                        dep_path = member_path

                result[dep_name] = Dependency(path=path_to_uri(dep_path))

                # Recursively parse dependency
                self._find_all_toml(dep_path, dep_name)

            elif isinstance(dep, dict) and DEP_GIT in dep:
                # Type 2: Git dependency
                git_path = self._get_path_by_lock_file(base_path, dep_name)

                if check_is_valid(git_path):
                    result[dep_name] = Dependency(path=path_to_uri(git_path))
                    self._find_all_toml(Path(git_path), dep_name)

            elif isinstance(dep, str):
                # Type 3: Version dependency
                version = dep
                repo_path = get_cjpm_config_path(CJPM_REPOSITORY_SUBDIR)
                dep_path = repo_path / f"{dep_name}-{version}"

                result[dep_name] = Dependency(path=path_to_uri(dep_path))
                self._find_all_toml(dep_path, dep_name)

        return result

    def _get_path_by_lock_file(self, base_path: Path, dep_name: str) -> str:
        """Resolve Git dependency path via cjpm.lock.

        Args:
            base_path: Base path containing cjpm.lock
            dep_name: Name of the dependency

        Returns:
            Resolved path string or empty string
        """
        git_dir = get_cjpm_config_path(CJPM_GIT_SUBDIR)
        lock_path = base_path / CJPM_LOCK

        lock_data: dict[str, Any] = {}

        # Parse cjpm.lock
        if lock_path.exists():
            lock_data = load_toml_safe(lock_path)

        # Fall back to cached root lock data
        requires = lock_data.get("requires", {})
        if dep_name not in requires:
            lock_data = self.root_module_lock_data
            requires = lock_data.get("requires", {})

        # Get commitId
        if dep_name in requires:
            dep_info = requires[dep_name]
            if isinstance(dep_info, dict) and "commitId" in dep_info:
                commit_id = dep_info["commitId"]

                # Cache lock data
                self.root_module_lock_data = lock_data

                # Return: ~/.cjpm/git/<depName>/<commitId>
                return str(git_dir / dep_name / commit_id)

        logger.warning(f"cjpm.lock not found or invalid for {dep_name}. Run cjpm update.")
        return ""

    def _is_workspace(self, dep_path: Path) -> bool:
        """Check if a path is a workspace root.

        Args:
            dep_path: Path to check

        Returns:
            True if path contains a workspace cjpm.toml
        """
        toml_path = dep_path / CJPM_TOML
        if not toml_path.exists():
            return False

        toml_obj = load_toml_safe(toml_path)
        return check_is_valid(toml_obj) and FIELD_WORKSPACE in toml_obj

    def _get_target_member_path(self, dep_name: str, workspace_path: Path) -> Path | None:
        """Find the member path matching a dependency name in a workspace.

        Args:
            dep_name: Name of the dependency to find
            workspace_path: Path to the workspace root

        Returns:
            Path to the matching member or None
        """
        if not check_is_valid(dep_name):
            return None

        toml_path = workspace_path / CJPM_TOML
        if not toml_path.exists():
            return None

        toml_obj = load_toml_safe(toml_path)
        if not check_is_valid(toml_obj):
            return None

        workspace = toml_obj.get(FIELD_WORKSPACE, {})
        if not isinstance(workspace, dict):
            return None
        members = self._get_members(workspace, workspace_path)

        for member_path in members:
            member_toml_path = member_path / CJPM_TOML
            if not member_toml_path.exists():
                continue

            member_toml = load_toml_safe(member_toml_path)
            if not check_is_valid(member_toml):
                continue

            pkg = member_toml.get(FIELD_PACKAGE, {})
            if FIELD_NAME not in pkg:
                continue

            if pkg[FIELD_NAME] == dep_name:
                return member_path

        return None

    def _get_java_modules(self, java_config: dict[str, Any]) -> list[str]:
        """Extract Java module names from FFI configuration.

        Args:
            java_config: Java FFI configuration section

        Returns:
            List of Java module names (keys from the config)
        """
        if not check_is_valid(java_config):
            return []

        # Return all keys as module names
        return list(java_config.keys())

    def _process_c_modules(self, c_config: dict[str, Any], module_path: Path) -> None:
        """Process C FFI modules (adds to require_path only).

        Args:
            c_config: C FFI configuration section
            module_path: Path to the module directory
        """
        for _module_name, module_config in c_config.items():
            if not isinstance(module_config, dict):
                continue

            if DEP_PATH in module_config:
                c_path_str = get_real_path(module_config[DEP_PATH])
                c_path = normalize_path(c_path_str, module_path)
                c_path_normalized = strip_trailing_separator(str(c_path))

                # Add to require_path (not in initOptions)
                self._add_to_require_path(c_path_normalized)

    def _add_to_require_path(self, lib_path: str) -> None:
        """Add a path to the require_path string.

        Args:
            lib_path: Path to add
        """
        if lib_path:
            separator = get_path_separator()
            self.require_path += lib_path + separator
