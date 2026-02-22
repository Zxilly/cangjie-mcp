use std::collections::HashMap;
use std::path::{Path, PathBuf};

use regex::Regex;
use serde::Deserialize;
use std::sync::LazyLock;

// -- URI / path conversion ---------------------------------------------------

pub fn path_to_uri(path: &Path) -> String {
    let path_str = path.to_string_lossy().replace('\\', "/");
    if cfg!(windows) {
        format!("file:///{path_str}")
    } else {
        format!("file://{path_str}")
    }
}

pub fn uri_to_path(uri: &str) -> PathBuf {
    if let Some(rest) = uri.strip_prefix("file:///") {
        if cfg!(windows) {
            PathBuf::from(rest.replace('/', "\\"))
        } else {
            PathBuf::from(format!("/{rest}"))
        }
    } else if let Some(rest) = uri.strip_prefix("file://") {
        PathBuf::from(rest)
    } else {
        PathBuf::from(uri)
    }
}

// -- Environment variable substitution ---------------------------------------

static ENV_VAR_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\$\{(\w+)\}").unwrap());

pub fn get_real_path(path_str: &str) -> String {
    if path_str.is_empty() {
        return path_str.to_string();
    }
    let normalized = path_str.replace('\\', "/");
    ENV_VAR_RE
        .replace_all(&normalized, |caps: &regex::Captures| {
            let var_name = &caps[1];
            std::env::var(var_name)
                .map(|v| v.replace('\\', "/"))
                .unwrap_or_else(|_| caps[0].to_string())
        })
        .to_string()
}

pub fn normalize_path(path_str: &str, base_path: &Path) -> PathBuf {
    let resolved = get_real_path(path_str);
    let path = PathBuf::from(&resolved);
    if path.is_absolute() {
        path
    } else {
        base_path.join(path)
    }
}

pub fn get_path_separator() -> &'static str {
    if cfg!(windows) {
        ";"
    } else {
        ":"
    }
}

pub fn strip_trailing_separator(path_str: &str) -> &str {
    path_str
        .strip_suffix('/')
        .or_else(|| path_str.strip_suffix('\\'))
        .unwrap_or(path_str)
}

pub fn merge_unique_strings(lists: &[&[String]]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();
    for list in lists {
        for item in *list {
            if seen.insert(item.clone()) {
                result.push(item.clone());
            }
        }
    }
    result
}

// -- CJPM TOML types --------------------------------------------------------

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
    fn test_path_to_uri_unix_style() {
        if !cfg!(windows) {
            assert_eq!(
                path_to_uri(Path::new("/home/user/file.cj")),
                "file:///home/user/file.cj"
            );
        }
    }

    #[test]
    fn test_path_to_uri_windows_style() {
        if cfg!(windows) {
            assert_eq!(
                path_to_uri(Path::new("C:\\Users\\test\\file.cj")),
                "file:///C:/Users/test/file.cj"
            );
        }
    }

    #[test]
    fn test_uri_to_path_file_triple_slash() {
        if cfg!(windows) {
            let path = uri_to_path("file:///C:/Users/test/file.cj");
            assert_eq!(path, PathBuf::from("C:\\Users\\test\\file.cj"));
        } else {
            let path = uri_to_path("file:///home/user/file.cj");
            assert_eq!(path, PathBuf::from("/home/user/file.cj"));
        }
    }

    #[test]
    fn test_uri_to_path_file_double_slash() {
        let path = uri_to_path("file:///tmp/test");
        if cfg!(windows) {
            assert_eq!(path, PathBuf::from("tmp\\test"));
        } else {
            assert_eq!(path, PathBuf::from("/tmp/test"));
        }
    }

    #[test]
    fn test_uri_to_path_plain() {
        let path = uri_to_path("/some/path");
        assert_eq!(path, PathBuf::from("/some/path"));
    }

    #[test]
    fn test_roundtrip_path_uri() {
        if cfg!(windows) {
            let original = Path::new("D:\\projects\\test\\main.cj");
            let uri = path_to_uri(original);
            let back = uri_to_path(&uri);
            assert_eq!(back, original);
        }
    }

    #[test]
    fn test_get_real_path_no_vars() {
        assert_eq!(get_real_path("/some/path"), "/some/path");
        assert_eq!(get_real_path(""), "");
    }

    #[test]
    fn test_get_real_path_with_env_var() {
        std::env::set_var("TEST_CANGJIE_VAR", "/resolved");
        assert_eq!(get_real_path("${TEST_CANGJIE_VAR}/sub"), "/resolved/sub");
        std::env::remove_var("TEST_CANGJIE_VAR");
    }

    #[test]
    fn test_get_real_path_unset_var_kept() {
        assert_eq!(
            get_real_path("${NONEXISTENT_VAR_12345}/sub"),
            "${NONEXISTENT_VAR_12345}/sub"
        );
    }

    #[test]
    fn test_normalize_path_absolute() {
        let base = Path::new("/base");
        let result = normalize_path("/absolute/path", base);
        assert_eq!(result, PathBuf::from("/absolute/path"));
    }

    #[test]
    fn test_normalize_path_relative() {
        let base = Path::new("/base/dir");
        let result = normalize_path("relative/path", base);
        assert_eq!(result, PathBuf::from("/base/dir/relative/path"));
    }

    #[test]
    fn test_get_path_separator() {
        if cfg!(windows) {
            assert_eq!(get_path_separator(), ";");
        } else {
            assert_eq!(get_path_separator(), ":");
        }
    }

    #[test]
    fn test_strip_trailing_separator() {
        assert_eq!(strip_trailing_separator("/path/"), "/path");
        assert_eq!(strip_trailing_separator("C:\\path\\"), "C:\\path");
        assert_eq!(strip_trailing_separator("/path"), "/path");
    }

    #[test]
    fn test_merge_unique_strings() {
        let a = vec!["a".to_string(), "b".to_string()];
        let b = vec!["b".to_string(), "c".to_string()];
        let result = merge_unique_strings(&[&a, &b]);
        assert_eq!(result, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_merge_unique_strings_empty() {
        let result = merge_unique_strings(&[]);
        assert!(result.is_empty());
    }

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
}
