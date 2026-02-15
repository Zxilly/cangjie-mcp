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
    CjpmBinDependencies,
    CjpmCModule,
    CjpmDepConfig,
    CjpmLock,
    CjpmTargetConfig,
    CjpmToml,
    CjpmWorkspace,
    get_cjpm_config_path,
    get_path_separator,
    get_real_path,
    load_cjpm_lock,
    load_cjpm_toml,
    merge_unique_strings,
    normalize_path,
    path_to_uri,
    strip_trailing_separator,
)

logger = logging.getLogger(__name__)

# TOML file names
CJPM_TOML = "cjpm.toml"
CJPM_LOCK = "cjpm.lock"


@dataclass
class Dependency:
    """A resolved dependency with file:// URI path."""

    path: str  # file:// URI format


@dataclass
class PackageRequires:
    """Binary dependency configuration."""

    package_option: dict[str, str] = field(default_factory=lambda: dict[str, str]())  # name -> file:// URI
    path_option: list[str] = field(default_factory=lambda: list[str]())  # file:// URI list


@dataclass
class ModuleOption:
    """Module configuration for LSP initialization."""

    name: str = ""
    requires: dict[str, Dependency] = field(default_factory=lambda: dict[str, Dependency]())
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
        self.root_module_lock_data: CjpmLock | None = None
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
        self.root_module_lock_data = None
        self.require_path = ""

    def _get_multi_module_option(self) -> None:
        """Detect workspace vs package mode and process accordingly."""
        cjpm = load_cjpm_toml(self.workspace_path / CJPM_TOML)
        if cjpm is None:
            self._process_package_mode()
            return

        # Validate: workspace and package cannot coexist at root
        if cjpm.workspace is not None and cjpm.package is not None:
            logger.warning("Both workspace and package fields found in cjpm.toml")
            return

        if cjpm.workspace is not None and cjpm.workspace.members:
            self._process_workspace_mode(cjpm)
        else:
            self._process_package_mode()

    def _process_workspace_mode(self, cjpm: CjpmToml) -> None:
        """Process workspace mode with member inheritance.

        Args:
            cjpm: Parsed root cjpm.toml
        """
        assert cjpm.workspace is not None

        # 1. Parse root-level dependencies (inherited by all members)
        root_requires = self._get_requires(cjpm.dependencies, self.workspace_path)

        # 2. Parse root-level target configuration
        root_package_requires = (
            self._get_targets_package_requires(cjpm.target, self.workspace_path) if cjpm.target else PackageRequires()
        )

        # 3. Process each member
        members = self._get_members(cjpm.workspace, self.workspace_path)
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

    def _get_members(self, workspace: CjpmWorkspace, base_path: Path) -> list[Path]:
        """Get valid member paths from workspace configuration.

        Args:
            workspace: Workspace configuration section
            base_path: Base path for resolving relative paths

        Returns:
            List of valid member paths
        """
        if not workspace.members:
            return []

        valid_paths: list[Path] = []
        invalid_paths: list[str] = []

        for member in workspace.members:
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

        cjpm = load_cjpm_toml(toml_path)

        # Validate TOML
        if cjpm is None:
            logger.warning(f"Invalid cjpm.toml in {module_uri}")
            self.multi_module_option[module_uri] = module_option
            return

        # Submodules cannot have workspace field
        if cjpm.workspace is not None:
            logger.warning(f"workspace field not allowed in {toml_path}")
            self.multi_module_option[module_uri] = module_option
            return

        # Get module name
        if cjpm.package is not None and cjpm.package.name:
            pkg_name = cjpm.package.name
            if expected_name and pkg_name != expected_name:
                logger.warning(f"Module name mismatch: expected {expected_name}, got {pkg_name}")
            module_option.name = pkg_name
        else:
            module_option.name = module_path.name

        # Parse dependencies
        self._find_dependencies(cjpm, module_option, module_path)

        self.multi_module_option[module_uri] = module_option

    def _find_dependencies(
        self,
        cjpm: CjpmToml,
        module_option: ModuleOption,
        module_path: Path,
    ) -> None:
        """Parse all dependency sections from a cjpm.toml.

        Args:
            cjpm: Parsed TOML configuration
            module_option: ModuleOption to populate
            module_path: Path to the module directory
        """
        # 1. Parse [target.*.bin-dependencies]
        if cjpm.target:
            if module_option.package_requires is None:
                module_option.package_requires = PackageRequires()

            target_pkg_reqs = self._get_targets_package_requires(cjpm.target, module_path)

            module_option.package_requires.package_option = {
                **module_option.package_requires.package_option,
                **target_pkg_reqs.package_option,
            }
            module_option.package_requires.path_option = merge_unique_strings(
                module_option.package_requires.path_option,
                target_pkg_reqs.path_option,
            )

        # 2. Parse [ffi]
        if cjpm.ffi is not None:
            # Java FFI
            if cjpm.ffi.java:
                module_option.java_requires = self._get_java_modules(cjpm.ffi.java)

            # C FFI (only adds to environment, not in initOptions)
            if cjpm.ffi.c:
                self._process_c_modules(cjpm.ffi.c, module_path)

        # 3. Parse [dependencies]
        if cjpm.dependencies:
            module_option.requires = self._get_requires(cjpm.dependencies, module_path)

        # 4. Parse [dev-dependencies]
        if cjpm.dev_dependencies:
            dev_requires = self._get_requires(cjpm.dev_dependencies, module_path)
            module_option.requires = {**module_option.requires, **dev_requires}

        # 5. Parse [target.*.dependencies] and [target.*.dev-dependencies]
        if cjpm.target:
            target_requires = self._get_targets_requires(cjpm.target, module_path)
            module_option.requires = {**module_option.requires, **target_requires}

    def _get_targets_package_requires(self, target: dict[str, CjpmTargetConfig], base_path: Path) -> PackageRequires:
        """Parse bin-dependencies from all target sections.

        Args:
            target: Target configuration section
            base_path: Base path for resolving paths

        Returns:
            Aggregated PackageRequires
        """
        result = PackageRequires()

        for _target_name, target_config in target.items():
            if target_config.bin_dependencies is not None:
                pkg_reqs = self._get_package_requires(target_config.bin_dependencies, base_path)
                result.package_option = {**result.package_option, **pkg_reqs.package_option}
                result.path_option = merge_unique_strings(result.path_option, pkg_reqs.path_option)

        return result

    def _get_package_requires(self, bin_deps: CjpmBinDependencies, base_path: Path) -> PackageRequires:
        """Parse a single bin-dependencies section.

        Args:
            bin_deps: bin-dependencies configuration
            base_path: Base path for resolving paths

        Returns:
            PackageRequires object
        """
        result = PackageRequires()

        # Process path-option array
        for p in bin_deps.path_option:
            lib_path = normalize_path(get_real_path(p), base_path)
            lib_path_str = strip_trailing_separator(str(lib_path))

            # Add to require_path
            self._add_to_require_path(lib_path_str)

            result.path_option.append(path_to_uri(lib_path_str))

        # Process package-option object
        for pkg_name, pkg_path in bin_deps.package_option.items():
            resolved_path = normalize_path(get_real_path(pkg_path), base_path)
            resolved_path_str = str(resolved_path)

            # Add parent directory to require_path
            self._add_to_require_path(str(resolved_path.parent))

            result.package_option[pkg_name] = path_to_uri(resolved_path_str)

        return result

    def _get_targets_requires(self, target: dict[str, CjpmTargetConfig], base_path: Path) -> dict[str, Dependency]:
        """Parse dependencies from all target sections.

        Args:
            target: Target configuration section
            base_path: Base path for resolving paths

        Returns:
            Aggregated dependencies
        """
        result: dict[str, Dependency] = {}

        for _target_name, target_config in target.items():
            # target.*.dependencies
            if target_config.dependencies:
                deps = self._get_requires(target_config.dependencies, base_path)
                result = {**result, **deps}

            # target.*.dev-dependencies
            if target_config.dev_dependencies:
                deps = self._get_requires(target_config.dev_dependencies, base_path)
                result = {**result, **deps}

        return result

    def _get_requires(self, dependencies: dict[str, str | CjpmDepConfig], base_path: Path) -> dict[str, Dependency]:
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
            if isinstance(dep, str):
                # Version dependency
                repo_path = get_cjpm_config_path(CJPM_REPOSITORY_SUBDIR)
                dep_path = repo_path / f"{dep_name}-{dep}"

                result[dep_name] = Dependency(path=path_to_uri(dep_path))
                self._find_all_toml(dep_path, dep_name)

            else:
                # dep is CjpmDepConfig (guaranteed by Pydantic)
                if dep.path is not None:
                    # Local path dependency
                    dep_path_str = get_real_path(dep.path)
                    dep_path = normalize_path(dep_path_str, base_path)

                    # Check if dependency is a workspace
                    if self._is_workspace(dep_path):
                        member_path = self._get_target_member_path(dep_name, dep_path)
                        if member_path:
                            dep_path = member_path

                    result[dep_name] = Dependency(path=path_to_uri(dep_path))

                    # Recursively parse dependency
                    self._find_all_toml(dep_path, dep_name)

                elif dep.git is not None:
                    # Git dependency
                    git_path = self._get_path_by_lock_file(base_path, dep_name)

                    if git_path:
                        result[dep_name] = Dependency(path=path_to_uri(git_path))
                        self._find_all_toml(Path(git_path), dep_name)

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

        # Parse cjpm.lock
        lock = load_cjpm_lock(lock_path) if lock_path.exists() else None

        # Fall back to cached root lock data
        if lock is None or dep_name not in lock.requires:
            lock = self.root_module_lock_data

        # Get commitId
        if lock is not None and dep_name in lock.requires:
            dep_info = lock.requires[dep_name]
            if dep_info.commitId:
                # Cache lock data
                self.root_module_lock_data = lock

                # Return: ~/.cjpm/git/<depName>/<commitId>
                return str(git_dir / dep_name / dep_info.commitId)

        logger.warning(f"cjpm.lock not found or invalid for {dep_name}. Run cjpm update.")
        return ""

    def _is_workspace(self, dep_path: Path) -> bool:
        """Check if a path is a workspace root.

        Args:
            dep_path: Path to check

        Returns:
            True if path contains a workspace cjpm.toml
        """
        cjpm = load_cjpm_toml(dep_path / CJPM_TOML)
        return cjpm is not None and cjpm.workspace is not None

    def _get_target_member_path(self, dep_name: str, workspace_path: Path) -> Path | None:
        """Find the member path matching a dependency name in a workspace.

        Args:
            dep_name: Name of the dependency to find
            workspace_path: Path to the workspace root

        Returns:
            Path to the matching member or None
        """
        if not dep_name:
            return None

        cjpm = load_cjpm_toml(workspace_path / CJPM_TOML)
        if cjpm is None or cjpm.workspace is None:
            return None

        members = self._get_members(cjpm.workspace, workspace_path)

        for member_path in members:
            member_toml_path = member_path / CJPM_TOML
            if not member_toml_path.exists():
                continue

            member_cjpm = load_cjpm_toml(member_toml_path)
            if member_cjpm is None or member_cjpm.package is None or not member_cjpm.package.name:
                continue

            if member_cjpm.package.name == dep_name:
                return member_path

        return None

    def _get_java_modules(self, java_config: dict[str, Any]) -> list[str]:
        """Extract Java module names from FFI configuration.

        Args:
            java_config: Java FFI configuration section

        Returns:
            List of Java module names (keys from the config)
        """
        if not java_config:
            return []

        # Return all keys as module names
        return list(java_config.keys())

    def _process_c_modules(self, c_modules: dict[str, CjpmCModule], module_path: Path) -> None:
        """Process C FFI modules (adds to require_path only).

        Args:
            c_modules: C FFI module configurations
            module_path: Path to the module directory
        """
        for _module_name, c_module in c_modules.items():
            if c_module.path:
                c_path_str = get_real_path(c_module.path)
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
