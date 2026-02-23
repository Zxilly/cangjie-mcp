use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Serialize;
use tracing::warn;

use crate::lsp::utils::*;

// -- Resolved types ----------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct Dependency {
    pub path: String, // file:// URI
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct PackageRequires {
    pub package_option: HashMap<String, String>,
    pub path_option: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ModuleOption {
    pub name: String,
    pub requires: HashMap<String, Dependency>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub package_requires: Option<PackageRequires>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub java_requires: Option<Vec<String>>,
}

// -- Dependency Resolver -----------------------------------------------------

pub struct DependencyResolver {
    workspace_path: PathBuf,
    multi_module_option: HashMap<String, ModuleOption>,
    existed: Vec<String>, // cycle detection
    root_lock_data: Option<CjpmLock>,
    require_path: String,
}

impl DependencyResolver {
    pub fn new(workspace_path: &Path) -> Self {
        Self {
            workspace_path: workspace_path.to_path_buf(),
            multi_module_option: HashMap::new(),
            existed: Vec::new(),
            root_lock_data: None,
            require_path: String::new(),
        }
    }

    pub fn resolve(&mut self) -> HashMap<String, ModuleOption> {
        self.clear_state();
        self.get_multi_module_option();
        self.multi_module_option.clone()
    }

    pub fn get_require_path(&self) -> &str {
        &self.require_path
    }

    fn clear_state(&mut self) {
        self.multi_module_option.clear();
        self.existed.clear();
        self.root_lock_data = None;
        self.require_path.clear();
    }

    fn get_multi_module_option(&mut self) {
        let toml_path = self.workspace_path.join(CJPM_TOML);
        let cjpm = match load_cjpm_toml(&toml_path) {
            Some(c) => c,
            None => {
                self.process_package_mode();
                return;
            }
        };

        if cjpm.workspace.is_some() && cjpm.package.is_some() {
            warn!("Both workspace and package fields found in cjpm.toml");
            return;
        }

        if let Some(ref ws) = cjpm.workspace {
            if !ws.members.is_empty() {
                self.process_workspace_mode(&cjpm);
                return;
            }
        }

        self.process_package_mode();
    }

    fn process_workspace_mode(&mut self, cjpm: &CjpmToml) {
        let ws = cjpm.workspace.as_ref().unwrap();
        let base = self.workspace_path.clone();

        let root_requires = self.get_requires(&cjpm.dependencies, &base);
        let root_pkg_requires = if !cjpm.target.is_empty() {
            self.get_targets_package_requires(&cjpm.target, &base)
        } else {
            PackageRequires::default()
        };

        let members = self.get_members(ws, &base);
        for member_path in &members {
            self.find_all_toml(member_path, "");
        }

        for member_path in &members {
            let member_uri = path_to_uri(member_path);
            if let Some(opt) = self.multi_module_option.get_mut(&member_uri) {
                // Merge root deps (root takes precedence)
                for (k, v) in &root_requires {
                    opt.requires.entry(k.clone()).or_insert_with(|| v.clone());
                }
                // Merge package requires
                let pkg_req = opt
                    .package_requires
                    .get_or_insert_with(PackageRequires::default);
                for (k, v) in &root_pkg_requires.package_option {
                    pkg_req
                        .package_option
                        .entry(k.clone())
                        .or_insert_with(|| v.clone());
                }
                let existing_paths = pkg_req.path_option.clone();
                pkg_req.path_option =
                    merge_unique_strings(&[&existing_paths, &root_pkg_requires.path_option]);
            }
        }
    }

    fn process_package_mode(&mut self) {
        let ws = self.workspace_path.clone();
        self.find_all_toml(&ws, "");
    }

    fn get_members(&self, workspace: &CjpmWorkspace, base_path: &Path) -> Vec<PathBuf> {
        let mut valid = Vec::new();
        for member in &workspace.members {
            let resolved = get_real_path(member);
            let path = normalize_path(&resolved, base_path);
            if path.exists() {
                valid.push(path);
            } else {
                warn!("Workspace member not found: {member}");
            }
        }
        valid
    }

    fn find_all_toml(&mut self, module_path: &Path, expected_name: &str) {
        let module_uri = path_to_uri(module_path);

        // Cycle detection
        if self.existed.contains(&module_uri) {
            return;
        }
        self.existed.push(module_uri.clone());

        let toml_path = module_path.join(CJPM_TOML);
        let mut module_option = ModuleOption::default();

        if !toml_path.exists() {
            self.multi_module_option.insert(module_uri, module_option);
            return;
        }

        let cjpm = match load_cjpm_toml(&toml_path) {
            Some(c) => c,
            None => {
                warn!("Invalid cjpm.toml in {module_uri}");
                self.multi_module_option.insert(module_uri, module_option);
                return;
            }
        };

        if cjpm.workspace.is_some() {
            warn!("workspace field not allowed in {}", toml_path.display());
            self.multi_module_option.insert(module_uri, module_option);
            return;
        }

        if let Some(ref pkg) = cjpm.package {
            if !pkg.name.is_empty() {
                if !expected_name.is_empty() && pkg.name != expected_name {
                    warn!(
                        "Module name mismatch: expected {expected_name}, got {}",
                        pkg.name
                    );
                }
                module_option.name = pkg.name.clone();
            } else {
                module_option.name = module_path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
            }
        } else {
            module_option.name = module_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
        }

        self.find_dependencies(&cjpm, &mut module_option, module_path);

        self.multi_module_option.insert(module_uri, module_option);
    }

    fn find_dependencies(
        &mut self,
        cjpm: &CjpmToml,
        module_option: &mut ModuleOption,
        module_path: &Path,
    ) {
        // Target bin-dependencies
        if !cjpm.target.is_empty() {
            let pkg_reqs = self.get_targets_package_requires(&cjpm.target, module_path);
            if module_option.package_requires.is_none() {
                module_option.package_requires = Some(PackageRequires::default());
            }
            if let Some(ref mut existing) = module_option.package_requires {
                existing.package_option.extend(pkg_reqs.package_option);
                let old = existing.path_option.clone();
                existing.path_option = merge_unique_strings(&[&old, &pkg_reqs.path_option]);
            }
        }

        // FFI
        if let Some(ref ffi) = cjpm.ffi {
            if !ffi.c.is_empty() {
                for c_module in ffi.c.values() {
                    if !c_module.path.is_empty() {
                        let resolved = normalize_path(&c_module.path, module_path);
                        let resolved_str = resolved.to_string_lossy().to_string();
                        if !self.require_path.contains(&resolved_str) {
                            let sep = get_path_separator();
                            if !self.require_path.is_empty() {
                                self.require_path.push_str(sep);
                            }
                            self.require_path.push_str(&resolved_str);
                        }
                    }
                }
            }
        }

        // Regular dependencies
        let requires = self.get_requires(&cjpm.dependencies, module_path);
        module_option.requires.extend(requires);

        // Dev dependencies
        let dev_requires = self.get_requires(&cjpm.dev_dependencies, module_path);
        module_option.requires.extend(dev_requires);

        // Target dependencies
        for target_config in cjpm.target.values() {
            let target_requires = self.get_requires(&target_config.dependencies, module_path);
            module_option.requires.extend(target_requires);
            let dev_target_requires =
                self.get_requires(&target_config.dev_dependencies, module_path);
            module_option.requires.extend(dev_target_requires);
        }
    }

    fn get_requires(
        &mut self,
        deps: &HashMap<String, CjpmDepValue>,
        module_path: &Path,
    ) -> HashMap<String, Dependency> {
        let mut result = HashMap::new();

        for (name, dep) in deps {
            match dep {
                CjpmDepValue::Config(config) => {
                    if let Some(ref path_str) = config.path {
                        // Local path dependency
                        let resolved = normalize_path(path_str, module_path);
                        let uri = path_to_uri(&resolved);
                        result.insert(name.clone(), Dependency { path: uri });
                        // Recurse
                        self.find_all_toml(&resolved, name);
                    } else if let Some(ref _git_url) = config.git {
                        // Git dependency — resolve via lock file
                        if let Some(dep) = self.resolve_git_dep(name, module_path) {
                            result.insert(name.clone(), dep);
                        }
                    }
                }
                CjpmDepValue::Version(version) => {
                    // Version dependency — resolve from ~/.cjpm/repository
                    if let Some(dep) = self.resolve_version_dep(name, version) {
                        result.insert(name.clone(), dep);
                    }
                }
            }
        }

        result
    }

    fn resolve_git_dep(&mut self, name: &str, _module_path: &Path) -> Option<Dependency> {
        if self.root_lock_data.is_none() {
            let lock_path = self.workspace_path.join(CJPM_LOCK);
            self.root_lock_data = load_cjpm_lock(&lock_path);
        }

        if let Some(ref lock) = self.root_lock_data {
            if let Some(req) = lock.requires.get(name) {
                if !req.commit_id.is_empty() {
                    let git_path = get_cjpm_config_path(CJPM_GIT_SUBDIR)
                        .join(name)
                        .join(&req.commit_id);
                    if git_path.exists() {
                        let uri = path_to_uri(&git_path);
                        self.find_all_toml(&git_path, name);
                        return Some(Dependency { path: uri });
                    }
                }
            }
        }

        None
    }

    fn resolve_version_dep(&mut self, name: &str, version: &str) -> Option<Dependency> {
        let repo_path = get_cjpm_config_path(CJPM_REPOSITORY_SUBDIR)
            .join(name)
            .join(version);
        if repo_path.exists() {
            let uri = path_to_uri(&repo_path);
            self.find_all_toml(&repo_path, name);
            Some(Dependency { path: uri })
        } else {
            None
        }
    }

    fn get_targets_package_requires(
        &self,
        targets: &HashMap<String, CjpmTargetConfig>,
        module_path: &Path,
    ) -> PackageRequires {
        let mut result = PackageRequires::default();

        for target_config in targets.values() {
            if let Some(ref bin_deps) = target_config.bin_dependencies {
                for (name, path_str) in &bin_deps.package_option {
                    let resolved = normalize_path(path_str, module_path);
                    let uri = path_to_uri(&resolved);
                    result.package_option.insert(name.clone(), uri);
                }
                for path_str in &bin_deps.path_option {
                    let resolved = normalize_path(path_str, module_path);
                    let uri = path_to_uri(&resolved);
                    if !result.path_option.contains(&uri) {
                        result.path_option.push(uri);
                    }
                }
            }
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// No cjpm.toml in the workspace directory -- resolve returns an empty map.
    #[test]
    fn test_resolver_no_cjpm_toml() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path().to_path_buf();

        let mut resolver = DependencyResolver::new(&ws);
        let modules = resolver.resolve();

        // process_package_mode -> find_all_toml on ws -> no cjpm.toml exists,
        // so a default ModuleOption is inserted with an empty name.
        // The single entry is keyed by the workspace URI.
        assert_eq!(modules.len(), 1);
        let uri = path_to_uri(&ws);
        let module = modules.get(&uri).expect("expected module at workspace URI");
        assert!(module.requires.is_empty());
    }

    /// Minimal cjpm.toml with just a [package] section.
    #[test]
    fn test_resolver_simple_package() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path().to_path_buf();

        std::fs::write(
            ws.join(CJPM_TOML),
            r#"
[package]
name = "myapp"
"#,
        )
        .unwrap();

        let mut resolver = DependencyResolver::new(&ws);
        let modules = resolver.resolve();

        assert_eq!(modules.len(), 1);
        let uri = path_to_uri(&ws);
        let module = modules.get(&uri).expect("expected module at workspace URI");
        assert_eq!(module.name, "myapp");
        assert!(module.requires.is_empty());
    }

    /// A package with a path dependency pointing to a subdirectory that also
    /// contains a cjpm.toml. Both modules should be resolved.
    #[test]
    fn test_resolver_with_path_dependency() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path().to_path_buf();

        // Root cjpm.toml depends on "childlib" via path.
        std::fs::write(
            ws.join(CJPM_TOML),
            r#"
[package]
name = "parent"

[dependencies.childlib]
path = "childlib"
"#,
        )
        .unwrap();

        // Create the child directory and its cjpm.toml.
        let child_dir = ws.join("childlib");
        std::fs::create_dir_all(&child_dir).unwrap();
        std::fs::write(
            child_dir.join(CJPM_TOML),
            r#"
[package]
name = "childlib"
"#,
        )
        .unwrap();

        let mut resolver = DependencyResolver::new(&ws);
        let modules = resolver.resolve();

        // Should have two modules: parent and childlib.
        assert_eq!(modules.len(), 2);

        let parent_uri = path_to_uri(&ws);
        let child_uri = path_to_uri(&child_dir);

        let parent = modules.get(&parent_uri).expect("parent module");
        assert_eq!(parent.name, "parent");
        assert!(parent.requires.contains_key("childlib"));
        assert_eq!(parent.requires["childlib"].path, child_uri);

        let child = modules.get(&child_uri).expect("child module");
        assert_eq!(child.name, "childlib");
    }

    /// Two packages that reference each other via path deps must not cause an
    /// infinite loop. Both should still appear in the resolved map.
    #[test]
    fn test_resolver_cycle_detection() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path().to_path_buf();

        let pkg_a_dir = ws.join("pkg_a");
        let pkg_b_dir = ws.join("pkg_b");
        std::fs::create_dir_all(&pkg_a_dir).unwrap();
        std::fs::create_dir_all(&pkg_b_dir).unwrap();

        // Root cjpm.toml -- workspace with two members is cleaner but we want
        // package-mode with a path dep that starts the chain.
        std::fs::write(
            ws.join(CJPM_TOML),
            r#"
[package]
name = "root"

[dependencies.pkg_a]
path = "pkg_a"
"#,
        )
        .unwrap();

        // pkg_a depends on pkg_b (uses relative path going up then into pkg_b).
        std::fs::write(
            pkg_a_dir.join(CJPM_TOML),
            r#"
[package]
name = "pkg_a"

[dependencies.pkg_b]
path = "../pkg_b"
"#,
        )
        .unwrap();

        // pkg_b depends back on pkg_a.
        std::fs::write(
            pkg_b_dir.join(CJPM_TOML),
            r#"
[package]
name = "pkg_b"

[dependencies.pkg_a]
path = "../pkg_a"
"#,
        )
        .unwrap();

        let mut resolver = DependencyResolver::new(&ws);
        let modules = resolver.resolve();

        // All three modules should be present despite the cycle.
        assert_eq!(modules.len(), 3);
        assert!(modules.contains_key(&path_to_uri(&ws)));
        assert!(modules.contains_key(&path_to_uri(&pkg_a_dir)));
        assert!(modules.contains_key(&path_to_uri(&pkg_b_dir)));
    }

    /// Workspace mode: cjpm.toml has a [workspace] with one member.
    #[test]
    fn test_resolver_workspace_mode() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path().to_path_buf();

        let pkg_a_dir = ws.join("pkg_a");
        std::fs::create_dir_all(&pkg_a_dir).unwrap();

        std::fs::write(
            ws.join(CJPM_TOML),
            r#"
[workspace]
members = ["pkg_a"]
"#,
        )
        .unwrap();

        std::fs::write(
            pkg_a_dir.join(CJPM_TOML),
            r#"
[package]
name = "pkg_a"
"#,
        )
        .unwrap();

        let mut resolver = DependencyResolver::new(&ws);
        let modules = resolver.resolve();

        let pkg_a_uri = path_to_uri(&pkg_a_dir);
        assert!(
            modules.contains_key(&pkg_a_uri),
            "modules should include pkg_a"
        );
        let module = modules.get(&pkg_a_uri).unwrap();
        assert_eq!(module.name, "pkg_a");
    }

    /// Dev-dependencies are merged into the module's requires.
    #[test]
    fn test_resolver_package_with_dev_dependencies() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path().to_path_buf();

        let dev_dep_dir = ws.join("testlib");
        std::fs::create_dir_all(&dev_dep_dir).unwrap();

        std::fs::write(
            ws.join(CJPM_TOML),
            r#"
[package]
name = "myapp"

[dev-dependencies.testlib]
path = "testlib"
"#,
        )
        .unwrap();

        std::fs::write(
            dev_dep_dir.join(CJPM_TOML),
            r#"
[package]
name = "testlib"
"#,
        )
        .unwrap();

        let mut resolver = DependencyResolver::new(&ws);
        let modules = resolver.resolve();

        let ws_uri = path_to_uri(&ws);
        let module = modules.get(&ws_uri).expect("root module");
        assert!(
            module.requires.contains_key("testlib"),
            "dev-dependencies should appear in requires"
        );
        let dep_uri = path_to_uri(&dev_dep_dir);
        assert_eq!(module.requires["testlib"].path, dep_uri);
    }

    /// When there are no FFI dependencies, get_require_path returns empty.
    #[test]
    fn test_resolver_get_require_path_empty() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path().to_path_buf();

        std::fs::write(
            ws.join(CJPM_TOML),
            r#"
[package]
name = "myapp"
"#,
        )
        .unwrap();

        let mut resolver = DependencyResolver::new(&ws);
        let _modules = resolver.resolve();

        assert!(
            resolver.get_require_path().is_empty(),
            "require_path should be empty when no FFI deps exist"
        );
    }

    /// FFI C dependency path is included in the resolver's require_path.
    #[test]
    fn test_resolver_ffi_c_dependency() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path().to_path_buf();

        let native_dir = ws.join("native").join("mylib");
        std::fs::create_dir_all(&native_dir).unwrap();

        std::fs::write(
            ws.join(CJPM_TOML),
            r#"
[package]
name = "myapp"

[ffi.c.mylib]
path = "native/mylib"
"#,
        )
        .unwrap();

        let mut resolver = DependencyResolver::new(&ws);
        let _modules = resolver.resolve();

        let require_path = resolver.get_require_path();
        let expected = native_dir.to_string_lossy().to_string();
        assert!(
            require_path.contains(&expected),
            "require_path should contain the resolved native path. \
             Got: {require_path}, expected to contain: {expected}"
        );
    }

    /// Both workspace and package fields in the same cjpm.toml triggers a
    /// warning and produces an empty result.
    #[test]
    fn test_resolver_both_workspace_and_package() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path().to_path_buf();

        std::fs::write(
            ws.join(CJPM_TOML),
            r#"
[package]
name = "myapp"

[workspace]
members = ["sub"]
"#,
        )
        .unwrap();

        let mut resolver = DependencyResolver::new(&ws);
        let modules = resolver.resolve();

        // Should return empty because both workspace and package are present
        assert!(
            modules.is_empty(),
            "modules should be empty when both workspace and package are present"
        );
    }

    /// Workspace with a non-existent member path should still resolve
    /// existing members.
    #[test]
    fn test_resolver_workspace_nonexistent_member() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path().to_path_buf();

        let pkg_a_dir = ws.join("pkg_a");
        std::fs::create_dir_all(&pkg_a_dir).unwrap();

        std::fs::write(
            ws.join(CJPM_TOML),
            r#"
[workspace]
members = ["pkg_a", "nonexistent_pkg"]
"#,
        )
        .unwrap();

        std::fs::write(
            pkg_a_dir.join(CJPM_TOML),
            r#"
[package]
name = "pkg_a"
"#,
        )
        .unwrap();

        let mut resolver = DependencyResolver::new(&ws);
        let modules = resolver.resolve();

        // Should have pkg_a but not nonexistent_pkg
        let pkg_a_uri = path_to_uri(&pkg_a_dir);
        assert!(
            modules.contains_key(&pkg_a_uri),
            "should include existing member pkg_a"
        );
    }

    /// Workspace mode with root dependencies that get merged into members.
    #[test]
    fn test_resolver_workspace_with_root_deps() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path().to_path_buf();

        let pkg_a_dir = ws.join("pkg_a");
        let lib_dir = ws.join("sharedlib");
        std::fs::create_dir_all(&pkg_a_dir).unwrap();
        std::fs::create_dir_all(&lib_dir).unwrap();

        // Root cjpm.toml: workspace with root-level dependencies
        std::fs::write(
            ws.join(CJPM_TOML),
            r#"
[workspace]
members = ["pkg_a"]

[dependencies.sharedlib]
path = "sharedlib"
"#,
        )
        .unwrap();

        std::fs::write(
            pkg_a_dir.join(CJPM_TOML),
            r#"
[package]
name = "pkg_a"
"#,
        )
        .unwrap();

        std::fs::write(
            lib_dir.join(CJPM_TOML),
            r#"
[package]
name = "sharedlib"
"#,
        )
        .unwrap();

        let mut resolver = DependencyResolver::new(&ws);
        let modules = resolver.resolve();

        let pkg_a_uri = path_to_uri(&pkg_a_dir);
        let module = modules.get(&pkg_a_uri).expect("pkg_a module");
        // Root deps should be merged into member
        assert!(
            module.requires.contains_key("sharedlib"),
            "root dependency should be merged into workspace member"
        );
    }

    /// Sub-module with a workspace field triggers a warning, default option inserted.
    #[test]
    fn test_resolver_submodule_with_workspace_field() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path().to_path_buf();

        let sub_dir = ws.join("submod");
        std::fs::create_dir_all(&sub_dir).unwrap();

        std::fs::write(
            ws.join(CJPM_TOML),
            r#"
[package]
name = "root"

[dependencies.submod]
path = "submod"
"#,
        )
        .unwrap();

        // Sub-module has workspace field (not allowed in sub-modules)
        std::fs::write(
            sub_dir.join(CJPM_TOML),
            r#"
[workspace]
members = ["something"]
"#,
        )
        .unwrap();

        let mut resolver = DependencyResolver::new(&ws);
        let modules = resolver.resolve();

        // Sub-module should still be in the map (default option inserted)
        let sub_uri = path_to_uri(&sub_dir);
        assert!(modules.contains_key(&sub_uri));
        let sub_module = modules.get(&sub_uri).unwrap();
        // Name should be empty since workspace field caused early return
        assert!(sub_module.name.is_empty() || sub_module.name == "submod");
    }

    /// Invalid cjpm.toml in sub-module still gets a default entry.
    #[test]
    fn test_resolver_submodule_invalid_cjpm_toml() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path().to_path_buf();

        let sub_dir = ws.join("badmod");
        std::fs::create_dir_all(&sub_dir).unwrap();

        std::fs::write(
            ws.join(CJPM_TOML),
            r#"
[package]
name = "root"

[dependencies.badmod]
path = "badmod"
"#,
        )
        .unwrap();

        // Invalid TOML content
        std::fs::write(sub_dir.join(CJPM_TOML), "invalid toml {{{").unwrap();

        let mut resolver = DependencyResolver::new(&ws);
        let modules = resolver.resolve();

        // badmod should still be present with a default module option
        let sub_uri = path_to_uri(&sub_dir);
        assert!(modules.contains_key(&sub_uri));
        let sub_module = modules.get(&sub_uri).unwrap();
        assert!(sub_module.requires.is_empty());
    }

    /// Package with empty name should use directory name instead.
    #[test]
    fn test_resolver_package_empty_name() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path().to_path_buf();

        std::fs::write(
            ws.join(CJPM_TOML),
            r#"
[package]
name = ""
"#,
        )
        .unwrap();

        let mut resolver = DependencyResolver::new(&ws);
        let modules = resolver.resolve();

        let uri = path_to_uri(&ws);
        let module = modules.get(&uri).expect("module");
        // Should fall back to directory name
        let dir_name = ws
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        assert_eq!(module.name, dir_name);
    }

    /// Package without a [package] section should use directory name.
    #[test]
    fn test_resolver_no_package_section_uses_dir_name() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path().to_path_buf();

        std::fs::write(
            ws.join(CJPM_TOML),
            r#"
[dependencies]
"#,
        )
        .unwrap();

        let mut resolver = DependencyResolver::new(&ws);
        let modules = resolver.resolve();

        let uri = path_to_uri(&ws);
        let module = modules.get(&uri).expect("module");
        let dir_name = ws
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        assert_eq!(module.name, dir_name);
    }

    /// Target dependencies are merged into the module's requires.
    #[test]
    fn test_resolver_target_dependencies() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path().to_path_buf();

        let target_dep_dir = ws.join("target_dep");
        std::fs::create_dir_all(&target_dep_dir).unwrap();

        std::fs::write(
            ws.join(CJPM_TOML),
            r#"
[package]
name = "myapp"

[target.x86_64.dependencies.target_dep]
path = "target_dep"
"#,
        )
        .unwrap();

        std::fs::write(
            target_dep_dir.join(CJPM_TOML),
            r#"
[package]
name = "target_dep"
"#,
        )
        .unwrap();

        let mut resolver = DependencyResolver::new(&ws);
        let modules = resolver.resolve();

        let uri = path_to_uri(&ws);
        let module = modules.get(&uri).expect("root module");
        assert!(
            module.requires.contains_key("target_dep"),
            "target dependencies should be in requires"
        );
    }

    /// Target dev-dependencies are merged into the module's requires.
    #[test]
    fn test_resolver_target_dev_dependencies() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path().to_path_buf();

        let dev_dep_dir = ws.join("target_dev_dep");
        std::fs::create_dir_all(&dev_dep_dir).unwrap();

        std::fs::write(
            ws.join(CJPM_TOML),
            r#"
[package]
name = "myapp"

[target.x86_64.dev-dependencies.target_dev_dep]
path = "target_dev_dep"
"#,
        )
        .unwrap();

        std::fs::write(
            dev_dep_dir.join(CJPM_TOML),
            r#"
[package]
name = "target_dev_dep"
"#,
        )
        .unwrap();

        let mut resolver = DependencyResolver::new(&ws);
        let modules = resolver.resolve();

        let uri = path_to_uri(&ws);
        let module = modules.get(&uri).expect("root module");
        assert!(
            module.requires.contains_key("target_dev_dep"),
            "target dev-dependencies should be in requires"
        );
    }

    /// bin-dependencies in target config should produce package_requires.
    #[test]
    fn test_resolver_bin_dependencies() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path().to_path_buf();

        let bin_dir = ws.join("binlib");
        std::fs::create_dir_all(&bin_dir).unwrap();

        std::fs::write(
            ws.join(CJPM_TOML),
            r#"
[package]
name = "myapp"

[target.default.bin-dependencies]
path-option = ["binlib"]

[target.default.bin-dependencies.package-option]
mybinlib = "binlib"
"#,
        )
        .unwrap();

        let mut resolver = DependencyResolver::new(&ws);
        let modules = resolver.resolve();

        let uri = path_to_uri(&ws);
        let module = modules.get(&uri).expect("root module");
        assert!(
            module.package_requires.is_some(),
            "package_requires should be set when bin-dependencies exist"
        );
        let pkg_reqs = module.package_requires.as_ref().unwrap();
        assert!(
            !pkg_reqs.package_option.is_empty(),
            "package_option should have entries"
        );
        assert!(
            !pkg_reqs.path_option.is_empty(),
            "path_option should have entries"
        );
    }

    /// Multiple FFI C dependencies should all appear in require_path.
    #[test]
    fn test_resolver_multiple_ffi_c_dependencies() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path().to_path_buf();

        let native1 = ws.join("native1");
        let native2 = ws.join("native2");
        std::fs::create_dir_all(&native1).unwrap();
        std::fs::create_dir_all(&native2).unwrap();

        std::fs::write(
            ws.join(CJPM_TOML),
            r#"
[package]
name = "myapp"

[ffi.c.lib1]
path = "native1"

[ffi.c.lib2]
path = "native2"
"#,
        )
        .unwrap();

        let mut resolver = DependencyResolver::new(&ws);
        let _modules = resolver.resolve();

        let require_path = resolver.get_require_path();
        let expected1 = native1.to_string_lossy().to_string();
        let expected2 = native2.to_string_lossy().to_string();
        assert!(
            require_path.contains(&expected1),
            "require_path should contain native1. Got: {require_path}"
        );
        assert!(
            require_path.contains(&expected2),
            "require_path should contain native2. Got: {require_path}"
        );
        // Should be separated by platform separator
        let sep = get_path_separator();
        assert!(
            require_path.contains(sep),
            "Multiple paths should be joined by separator"
        );
    }

    /// FFI C dependency with empty path should not add to require_path.
    #[test]
    fn test_resolver_ffi_c_empty_path() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path().to_path_buf();

        std::fs::write(
            ws.join(CJPM_TOML),
            r#"
[package]
name = "myapp"

[ffi.c.mylib]
path = ""
"#,
        )
        .unwrap();

        let mut resolver = DependencyResolver::new(&ws);
        let _modules = resolver.resolve();

        assert!(
            resolver.get_require_path().is_empty(),
            "require_path should be empty when FFI path is empty"
        );
    }

    /// Git dependency without lock file resolves to None.
    #[test]
    fn test_resolver_git_dep_no_lock_file() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path().to_path_buf();

        std::fs::write(
            ws.join(CJPM_TOML),
            r#"
[package]
name = "myapp"

[dependencies.gitlib]
git = "https://example.com/gitlib.git"
"#,
        )
        .unwrap();

        let mut resolver = DependencyResolver::new(&ws);
        let modules = resolver.resolve();

        let uri = path_to_uri(&ws);
        let module = modules.get(&uri).expect("root module");
        // Without a lock file, git dep should not be resolved
        assert!(
            !module.requires.contains_key("gitlib"),
            "git dependency should not be resolved without a lock file"
        );
    }

    /// Git dependency with a lock file but non-existent git checkout path
    /// should not produce a resolved dependency.
    #[test]
    fn test_resolver_git_dep_with_lock_but_no_checkout() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path().to_path_buf();

        std::fs::write(
            ws.join(CJPM_TOML),
            r#"
[package]
name = "myapp"

[dependencies.gitlib]
git = "https://example.com/gitlib.git"
"#,
        )
        .unwrap();

        // Create a lock file with commit ID
        std::fs::write(
            ws.join(CJPM_LOCK),
            r#"
[requires.gitlib]
commitId = "deadbeef123"
"#,
        )
        .unwrap();

        let mut resolver = DependencyResolver::new(&ws);
        let modules = resolver.resolve();

        let uri = path_to_uri(&ws);
        let module = modules.get(&uri).expect("root module");
        // The git path won't exist on disk, so dep won't resolve
        assert!(
            !module.requires.contains_key("gitlib"),
            "git dep should not resolve when checkout dir doesn't exist"
        );
    }

    /// Version dependency with non-existent repository path should not resolve.
    #[test]
    fn test_resolver_version_dep_no_repo() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path().to_path_buf();

        std::fs::write(
            ws.join(CJPM_TOML),
            r#"
[package]
name = "myapp"

[dependencies]
somelib = "1.0.0"
"#,
        )
        .unwrap();

        let mut resolver = DependencyResolver::new(&ws);
        let modules = resolver.resolve();

        let uri = path_to_uri(&ws);
        let module = modules.get(&uri).expect("root module");
        // Without ~/.cjpm/repository/somelib/1.0.0, this won't resolve
        assert!(
            !module.requires.contains_key("somelib"),
            "version dep should not resolve when repo path doesn't exist"
        );
    }

    /// resolve() can be called multiple times and produces fresh results.
    #[test]
    fn test_resolver_clear_state_on_resolve() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path().to_path_buf();

        std::fs::write(
            ws.join(CJPM_TOML),
            r#"
[package]
name = "myapp"
"#,
        )
        .unwrap();

        let mut resolver = DependencyResolver::new(&ws);

        // First resolve
        let modules1 = resolver.resolve();
        assert_eq!(modules1.len(), 1);

        // Second resolve should produce same results (state is cleared)
        let modules2 = resolver.resolve();
        assert_eq!(modules2.len(), 1);
        assert_eq!(
            modules1.keys().collect::<Vec<_>>(),
            modules2.keys().collect::<Vec<_>>()
        );
    }

    /// Workspace mode with root dependencies AND targets with bin-dependencies
    /// should merge both into members.
    #[test]
    fn test_resolver_workspace_with_root_targets() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path().to_path_buf();

        let pkg_a_dir = ws.join("pkg_a");
        let bin_dir = ws.join("binlib");
        std::fs::create_dir_all(&pkg_a_dir).unwrap();
        std::fs::create_dir_all(&bin_dir).unwrap();

        std::fs::write(
            ws.join(CJPM_TOML),
            r#"
[workspace]
members = ["pkg_a"]

[target.default.bin-dependencies]
path-option = ["binlib"]

[target.default.bin-dependencies.package-option]
mybinlib = "binlib"
"#,
        )
        .unwrap();

        std::fs::write(
            pkg_a_dir.join(CJPM_TOML),
            r#"
[package]
name = "pkg_a"
"#,
        )
        .unwrap();

        let mut resolver = DependencyResolver::new(&ws);
        let modules = resolver.resolve();

        let pkg_a_uri = path_to_uri(&pkg_a_dir);
        let module = modules.get(&pkg_a_uri).expect("pkg_a module");
        // Root target bin-dependencies should be merged into member's package_requires
        assert!(
            module.package_requires.is_some(),
            "package_requires should be merged from root targets"
        );
    }

    /// Name mismatch between expected name and package name logs a warning
    /// but still uses the package name.
    #[test]
    fn test_resolver_name_mismatch() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path().to_path_buf();

        let sub_dir = ws.join("mylib");
        std::fs::create_dir_all(&sub_dir).unwrap();

        std::fs::write(
            ws.join(CJPM_TOML),
            r#"
[package]
name = "root"

[dependencies.mylib]
path = "mylib"
"#,
        )
        .unwrap();

        // The dependency key is "mylib" but the package declares "different_name"
        std::fs::write(
            sub_dir.join(CJPM_TOML),
            r#"
[package]
name = "different_name"
"#,
        )
        .unwrap();

        let mut resolver = DependencyResolver::new(&ws);
        let modules = resolver.resolve();

        let sub_uri = path_to_uri(&sub_dir);
        let sub_module = modules.get(&sub_uri).expect("sub module");
        // Should use the actual package name from cjpm.toml, not the key
        assert_eq!(sub_module.name, "different_name");
    }

    /// Workspace with empty members list should fall through to package mode.
    #[test]
    fn test_resolver_workspace_empty_members() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path().to_path_buf();

        std::fs::write(
            ws.join(CJPM_TOML),
            r#"
[workspace]
members = []
"#,
        )
        .unwrap();

        let mut resolver = DependencyResolver::new(&ws);
        let modules = resolver.resolve();

        // Empty members -> falls through to process_package_mode
        // which calls find_all_toml on workspace root
        let uri = path_to_uri(&ws);
        assert!(modules.contains_key(&uri));
    }

    /// Git dependency with lock file that has empty commitId should not resolve.
    #[test]
    fn test_resolver_git_dep_empty_commit_id() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path().to_path_buf();

        std::fs::write(
            ws.join(CJPM_TOML),
            r#"
[package]
name = "myapp"

[dependencies.gitlib]
git = "https://example.com/gitlib.git"
"#,
        )
        .unwrap();

        // Lock file with empty commitId
        std::fs::write(
            ws.join(CJPM_LOCK),
            r#"
[requires.gitlib]
commitId = ""
"#,
        )
        .unwrap();

        let mut resolver = DependencyResolver::new(&ws);
        let modules = resolver.resolve();

        let uri = path_to_uri(&ws);
        let module = modules.get(&uri).expect("root module");
        assert!(
            !module.requires.contains_key("gitlib"),
            "git dep with empty commitId should not resolve"
        );
    }
}
