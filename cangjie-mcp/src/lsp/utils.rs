use std::collections::HashMap;
use std::path::{Path, PathBuf};

use regex::Regex;
use serde::Deserialize;
use std::sync::LazyLock;

// -- URI / path conversion ---------------------------------------------------
// Aligned with Cangjie LSP server C++ implementation:
// - URI.cpp: URIFromAbsolutePath, ToString, PercentEncode, ShouldEscape
// - Utils.cpp: PathWindowsToLinux

pub fn path_to_uri(path: &Path) -> String {
    let s = path.to_string_lossy();
    let body = path_to_uri_body(&s);
    let encoded = percent_encode_uri_body(&body);
    format!("file://{encoded}")
}

pub fn uri_to_path(uri: &str) -> PathBuf {
    let path_str = if let Some(rest) = uri.strip_prefix("file:///") {
        if cfg!(windows) {
            let decoded = percent_decode(rest);
            decoded.replace('/', "\\")
        } else {
            format!("/{}", percent_decode(rest))
        }
    } else if let Some(rest) = uri.strip_prefix("file://") {
        percent_decode(rest)
    } else {
        uri.to_string()
    };
    PathBuf::from(path_str)
}

/// Build the URI body from a filesystem path (before percent-encoding).
/// Converts backslashes to forward slashes and prepends '/' for Windows drive paths.
fn path_to_uri_body(path: &str) -> String {
    // 1. Backslash → forward slash (C++ PathWindowsToLinux)
    let path = path.replace('\\', "/");
    // 2. Windows drive letter path (e.g., D:/foo): prepend '/'
    if path.len() > 1 && path.as_bytes().get(1) == Some(&b':') {
        format!("/{path}")
    } else {
        path
    }
}

/// Percent-encode a URI body. Only unreserved characters (RFC 3986) and '/' are
/// kept literal; everything else (including ':') is encoded as %XX with uppercase
/// hex digits — matching the C++ PercentEncode + ShouldEscape.
fn percent_encode_uri_body(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + s.len() / 4);
    for b in s.bytes() {
        if is_uri_unreserved(b) {
            out.push(b as char);
        } else {
            out.push('%');
            out.push(hex_upper(b >> 4));
            out.push(hex_upper(b & 0x0F));
        }
    }
    out
}

/// Unreserved characters that are never percent-encoded in URI body.
/// Matches C++ ShouldEscape: a-z A-Z 0-9 - _ . ~ / are NOT escaped.
fn is_uri_unreserved(b: u8) -> bool {
    matches!(b, b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/')
}

fn hex_upper(n: u8) -> char {
    const HEX: [u8; 16] = *b"0123456789ABCDEF";
    HEX[n as usize] as char
}

/// Decode percent-encoded sequences (%XX) in a string.
fn percent_decode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(hi), Some(lo)) = (from_hex(bytes[i + 1]), from_hex(bytes[i + 2])) {
                out.push((hi << 4 | lo) as char);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn from_hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
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
    let joined = if path.is_absolute() {
        path
    } else {
        base_path.join(path)
    };
    clean_path_components(&joined)
}

/// Normalize a path by resolving `.` and `..` components without filesystem access.
fn clean_path_components(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut parts: Vec<Component<'_>> = Vec::new();
    for c in path.components() {
        match c {
            Component::ParentDir => {
                if matches!(parts.last(), Some(Component::Normal(_))) {
                    parts.pop();
                } else {
                    parts.push(c);
                }
            }
            Component::CurDir => {}
            _ => parts.push(c),
        }
    }
    parts.iter().collect()
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
                "file:///C%3A/Users/test/file.cj"
            );
        }
    }

    #[test]
    fn test_uri_to_path_file_triple_slash() {
        if cfg!(windows) {
            // Percent-encoded colon (standard LSP format)
            let path = uri_to_path("file:///c%3A/Users/test/file.cj");
            assert_eq!(path, PathBuf::from("c:\\Users\\test\\file.cj"));
            // Unencoded colon (also handled)
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
            assert_eq!(uri, "file:///D%3A/projects/test/main.cj");
            let back = uri_to_path(&uri);
            // Drive letter case is preserved through the roundtrip
            assert_eq!(back, PathBuf::from("D:\\projects\\test\\main.cj"));
        }
    }

    #[test]
    fn test_get_real_path_no_vars() {
        assert_eq!(get_real_path("/some/path"), "/some/path");
        assert_eq!(get_real_path(""), "");
    }

    #[test]
    fn test_get_real_path_with_env_var() {
        temp_env::with_var("TEST_CANGJIE_VAR", Some("/resolved"), || {
            assert_eq!(get_real_path("${TEST_CANGJIE_VAR}/sub"), "/resolved/sub");
        });
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
        // Without CJPM_CONFIG set, should use home dir
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

    #[test]
    fn test_get_real_path_backslash_normalized() {
        let result = get_real_path("C:\\Users\\test\\file.cj");
        assert_eq!(result, "C:/Users/test/file.cj");
    }

    #[test]
    fn test_merge_unique_strings_preserves_order() {
        let a = vec!["c".to_string(), "a".to_string()];
        let b = vec!["b".to_string(), "a".to_string()];
        let result = merge_unique_strings(&[&a, &b]);
        assert_eq!(result, vec!["c", "a", "b"]);
    }
}
