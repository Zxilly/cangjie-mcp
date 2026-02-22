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

        // Get module name
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

        // Parse dependencies
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
        // Load lock file if not cached
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
