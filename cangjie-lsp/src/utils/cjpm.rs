use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

pub const CJPM_DEFAULT_DIR: &str = ".cjpm";
pub const CJPM_GIT_SUBDIR: &str = "git";
pub const CJPM_REPOSITORY_SUBDIR: &str = "repository";
pub const CJPM_TOML: &str = "cjpm.toml";
pub const CJPM_LOCK: &str = "cjpm.lock";

pub fn get_cjpm_config_path(subdir: &str) -> PathBuf {
    if let Ok(config) = std::env::var("CJPM_CONFIG") {
        return PathBuf::from(config).join(subdir);
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(CJPM_DEFAULT_DIR)
        .join(subdir)
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct CjpmPackage {
    #[serde(default)]
    pub name: String,
    #[serde(default, rename = "target-dir")]
    pub target_dir: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct CjpmWorkspace {
    #[serde(default)]
    pub members: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum CjpmDepValue {
    Version(String),
    Config(CjpmDepConfig),
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct CjpmDepConfig {
    pub path: Option<String>,
    pub git: Option<String>,
    pub tag: Option<String>,
    pub branch: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct CjpmBinDependencies {
    #[serde(default, rename = "path-option")]
    pub path_option: Vec<String>,
    #[serde(default, rename = "package-option")]
    pub package_option: HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct CjpmTargetConfig {
    #[serde(default)]
    pub dependencies: HashMap<String, CjpmDepValue>,
    #[serde(default, rename = "dev-dependencies")]
    pub dev_dependencies: HashMap<String, CjpmDepValue>,
    #[serde(default, rename = "bin-dependencies")]
    pub bin_dependencies: Option<CjpmBinDependencies>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct CjpmCModule {
    #[serde(default)]
    pub path: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct CjpmFfi {
    #[serde(default)]
    pub java: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub c: HashMap<String, CjpmCModule>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct CjpmToml {
    pub package: Option<CjpmPackage>,
    pub workspace: Option<CjpmWorkspace>,
    #[serde(default)]
    pub dependencies: HashMap<String, CjpmDepValue>,
    #[serde(default, rename = "dev-dependencies")]
    pub dev_dependencies: HashMap<String, CjpmDepValue>,
    #[serde(default)]
    pub target: HashMap<String, CjpmTargetConfig>,
    pub ffi: Option<CjpmFfi>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct CjpmLockRequire {
    #[serde(default, rename = "commitId")]
    pub commit_id: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct CjpmLock {
    #[serde(default)]
    pub requires: HashMap<String, CjpmLockRequire>,
}

pub fn load_cjpm_toml(toml_path: &Path) -> Option<CjpmToml> {
    if !toml_path.exists() {
        return None;
    }
    let content = std::fs::read_to_string(toml_path).ok()?;
    toml::from_str(&content).ok()
}

pub fn load_cjpm_lock(lock_path: &Path) -> Option<CjpmLock> {
    if !lock_path.exists() {
        return None;
    }
    let content = std::fs::read_to_string(lock_path).ok()?;
    toml::from_str(&content).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_cjpm_toml() {
        let toml_str = r#"
[package]
name = "my-project"

[dependencies]
std = "0.55.3"
"#;
        let parsed: CjpmToml = toml::from_str(toml_str).unwrap();
        assert_eq!(parsed.package.unwrap().name, "my-project");
        assert!(parsed.dependencies.contains_key("std"));
    }

    #[test]
    fn test_parse_cjpm_toml_workspace() {
        let toml_str = r#"
[workspace]
members = ["lib", "app"]
"#;
        let parsed: CjpmToml = toml::from_str(toml_str).unwrap();
        let ws = parsed.workspace.unwrap();
        assert_eq!(ws.members, vec!["lib", "app"]);
    }

    #[test]
    fn test_parse_cjpm_dep_config() {
        let toml_str = r#"
[dependencies]
simple = "1.0"

[dependencies.complex]
path = "../lib"
"#;
        let parsed: CjpmToml = toml::from_str(toml_str).unwrap();
        match &parsed.dependencies["simple"] {
            CjpmDepValue::Version(v) => assert_eq!(v, "1.0"),
            _ => panic!("Expected version string"),
        }
        match &parsed.dependencies["complex"] {
            CjpmDepValue::Config(c) => assert_eq!(c.path.as_deref(), Some("../lib")),
            _ => panic!("Expected config"),
        }
    }

    #[test]
    fn test_load_cjpm_toml_nonexistent() {
        let result = load_cjpm_toml(Path::new("/nonexistent/cjpm.toml"));
        assert!(result.is_none());
    }

    #[test]
    fn test_load_cjpm_toml_valid_file() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(
            tmp.path(),
            r#"
[package]
name = "test-project"

[dependencies]
std = "0.55"
"#,
        )
        .unwrap();
        let result = load_cjpm_toml(tmp.path());
        assert!(result.is_some());
        let toml = result.unwrap();
        assert_eq!(toml.package.unwrap().name, "test-project");
    }

    #[test]
    fn test_load_cjpm_toml_invalid_content() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), "not valid toml {{{").unwrap();
        let result = load_cjpm_toml(tmp.path());
        assert!(result.is_none());
    }

    #[test]
    fn test_load_cjpm_lock_nonexistent() {
        let result = load_cjpm_lock(Path::new("/nonexistent/cjpm.lock"));
        assert!(result.is_none());
    }

    #[test]
    fn test_load_cjpm_lock_valid_file() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(
            tmp.path(),
            r#"
[requires.some_dep]
commitId = "abc123"
"#,
        )
        .unwrap();
        let result = load_cjpm_lock(tmp.path());
        assert!(result.is_some());
        let lock = result.unwrap();
        assert_eq!(lock.requires["some_dep"].commit_id, "abc123");
    }

    #[test]
    fn test_load_cjpm_lock_invalid_content() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), "invalid {{{ toml").unwrap();
        let result = load_cjpm_lock(tmp.path());
        assert!(result.is_none());
    }

    #[test]
    fn test_get_cjpm_config_path_default() {
        temp_env::with_var("CJPM_CONFIG", None::<&str>, || {
            let path = get_cjpm_config_path("git");
            assert!(path.to_string_lossy().contains(".cjpm"));
            assert!(path.to_string_lossy().ends_with("git"));
        });
    }

    #[test]
    fn test_get_cjpm_config_path_custom() {
        temp_env::with_var("CJPM_CONFIG", Some("/custom/config"), || {
            let path = get_cjpm_config_path("repository");
            assert_eq!(path, PathBuf::from("/custom/config/repository"));
        });
    }

    #[test]
    fn test_parse_cjpm_toml_with_git_dep() {
        let toml_str = r#"
[dependencies.mylib]
git = "https://github.com/example/mylib.git"
"#;
        let parsed: CjpmToml = toml::from_str(toml_str).unwrap();
        match &parsed.dependencies["mylib"] {
            CjpmDepValue::Config(c) => {
                assert_eq!(
                    c.git.as_deref(),
                    Some("https://github.com/example/mylib.git")
                );
                assert!(c.path.is_none());
            }
            _ => panic!("Expected config with git"),
        }
    }

    #[test]
    fn test_parse_cjpm_toml_with_target_deps() {
        let toml_str = r#"
[target.x86_64.dependencies]
native = "1.0"

[target.x86_64.dev-dependencies]
test-native = "0.1"
"#;
        let parsed: CjpmToml = toml::from_str(toml_str).unwrap();
        let target = &parsed.target["x86_64"];
        assert!(target.dependencies.contains_key("native"));
        assert!(target.dev_dependencies.contains_key("test-native"));
    }

    #[test]
    fn test_parse_cjpm_lock_multiple_requires() {
        let toml_str = r#"
[requires.dep_a]
commitId = "aaa111"

[requires.dep_b]
commitId = "bbb222"
"#;
        let parsed: CjpmLock = toml::from_str(toml_str).unwrap();
        assert_eq!(parsed.requires.len(), 2);
        assert_eq!(parsed.requires["dep_a"].commit_id, "aaa111");
        assert_eq!(parsed.requires["dep_b"].commit_id, "bbb222");
    }
}
