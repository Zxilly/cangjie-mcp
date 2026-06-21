use super::*;
use tempfile::TempDir;

/// No cjpm.toml in the workspace directory -- resolve returns an empty map.
#[test]
fn test_resolver_no_cjpm_toml() {
    let tmp = TempDir::new().unwrap();
    let ws = tmp.path().to_path_buf();

    let mut resolver = DependencyResolver::new(&ws);
    let modules = resolver.resolve();

    // No cjpm.toml: a default ModuleOption keyed by the workspace URI is inserted.
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

/// A package with a path dependency to a subdir that also has a cjpm.toml;
/// both modules should resolve.
#[test]
fn test_resolver_with_path_dependency() {
    let tmp = TempDir::new().unwrap();
    let ws = tmp.path().to_path_buf();

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

    // All three present despite the cycle.
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

    let sub_uri = path_to_uri(&sub_dir);
    assert!(modules.contains_key(&sub_uri));
    let sub_module = modules.get(&sub_uri).unwrap();
    // Name stays empty because the workspace field caused an early return.
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

    std::fs::write(sub_dir.join(CJPM_TOML), "invalid toml {{{").unwrap();

    let mut resolver = DependencyResolver::new(&ws);
    let modules = resolver.resolve();

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
    let path_count = std::env::split_paths(require_path).count();
    assert!(
        path_count >= 2,
        "Multiple paths should be represented as multiple PATH entries"
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
    // The git checkout path won't exist on disk, so the dep won't resolve.
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
    // No ~/.cjpm/repository/somelib/1.0.0 on disk, so this won't resolve.
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

    let modules1 = resolver.resolve();
    assert_eq!(modules1.len(), 1);

    // Second resolve clears state and produces the same result.
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

    // Dep key is "mylib" but the package declares "different_name".
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

    // Empty members falls through to package mode (find_all_toml on the ws root).
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
