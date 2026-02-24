use std::collections::HashMap;
use std::path::PathBuf;

use crate::lsp::dependency::DependencyResolver;
use serde::Serialize;

// -- LSP Settings ------------------------------------------------------------

pub struct LSPSettings {
    pub sdk_path: PathBuf,
    pub workspace_path: PathBuf,
    pub log_enabled: bool,
    pub log_path: Option<PathBuf>,
    pub init_timeout_ms: u64,
    pub disable_auto_import: bool,
}

impl LSPSettings {
    pub fn get_lsp_args(&self) -> Vec<String> {
        let mut args = vec!["src".to_string()];

        if self.disable_auto_import {
            args.push("--disableAutoImport".to_string());
        }

        if self.log_enabled {
            if let Some(ref log_path) = self.log_path {
                args.push("-V".to_string());
                args.push("--enable-log=true".to_string());
                args.push(format!("--log-path={}", log_path.display()));
            }
        } else {
            args.push("--enable-log=false".to_string());
        }

        args
    }

    pub fn lsp_server_path(&self) -> PathBuf {
        let exe_name = if cfg!(windows) {
            "LSPServer.exe"
        } else {
            "LSPServer"
        };
        self.sdk_path.join("tools").join("bin").join(exe_name)
    }

    pub fn envsetup_script_path(&self) -> PathBuf {
        if cfg!(windows) {
            self.sdk_path.join("envsetup.ps1")
        } else {
            self.sdk_path.join("envsetup.sh")
        }
    }

    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();

        if !self.sdk_path.exists() {
            errors.push(format!(
                "SDK path does not exist: {}",
                self.sdk_path.display()
            ));
        }

        let server_path = self.lsp_server_path();
        if !server_path.exists() {
            errors.push(format!("LSP server not found: {}", server_path.display()));
        }

        let envsetup_path = self.envsetup_script_path();
        if !envsetup_path.exists() {
            errors.push(format!(
                "Environment setup script not found: {}",
                envsetup_path.display()
            ));
        }

        if !self.workspace_path.exists() {
            errors.push(format!(
                "Workspace path does not exist: {}",
                self.workspace_path.display()
            ));
        }

        errors
    }
}

// -- LSP Init Options --------------------------------------------------------

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LSPInitOptions {
    #[serde(default)]
    pub multi_module_option: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub condition_compile_option: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub single_condition_compile_option: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub condition_compile_paths: Vec<String>,
    #[serde(default)]
    pub target_lib: String,
    #[serde(default)]
    pub modules_home_option: String,
    #[serde(default)]
    pub std_lib_path_option: String,
    #[serde(default)]
    pub telemetry_option: bool,
    #[serde(default)]
    pub extension_path: String,
}

// -- Build init options ------------------------------------------------------

pub fn build_init_options(settings: &LSPSettings) -> (LSPInitOptions, String) {
    let mut resolver = DependencyResolver::new(&settings.workspace_path);
    let multi_module = resolver.resolve();

    let multi_module_option: HashMap<String, serde_json::Value> = multi_module
        .into_iter()
        .filter_map(|(k, v)| serde_json::to_value(v).ok().map(|val| (k, val)))
        .collect();

    let require_path = resolver.get_require_path().to_string();

    // Must be non-empty: the LSP server's IsInCjlibDir() uses
    // std::string::find(stdLibPath) — an empty string matches every path,
    // causing all document requests to be treated as stdlib files and
    // suppressing responses.
    let std_lib_path = settings.sdk_path.join("lib").to_string_lossy().to_string();

    // The LSP server uses targetLib as a cache directory for compilation
    // artifacts. Without it, incremental compilation and AST caching may fail.
    let target_lib = settings
        .workspace_path
        .join(".cache")
        .join("lsp")
        .to_string_lossy()
        .to_string();

    let options = LSPInitOptions {
        multi_module_option,
        std_lib_path_option: std_lib_path,
        modules_home_option: settings.sdk_path.to_string_lossy().to_string(),
        target_lib,
        ..Default::default()
    };

    (options, require_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_lsp_settings() -> LSPSettings {
        LSPSettings {
            sdk_path: PathBuf::from("/opt/cangjie-sdk"),
            workspace_path: PathBuf::from("/tmp/test-workspace"),
            log_enabled: false,
            log_path: None,
            init_timeout_ms: 30000,
            disable_auto_import: false,
        }
    }

    #[test]
    fn test_get_lsp_args_default() {
        let settings = test_lsp_settings();
        let args = settings.get_lsp_args();
        assert!(args.contains(&"src".to_string()));
        assert!(args.contains(&"--enable-log=false".to_string()));
        assert!(!args.contains(&"--disableAutoImport".to_string()));
    }

    #[test]
    fn test_get_lsp_args_with_auto_import_disabled() {
        let mut settings = test_lsp_settings();
        settings.disable_auto_import = true;
        let args = settings.get_lsp_args();
        assert!(args.contains(&"--disableAutoImport".to_string()));
    }

    #[test]
    fn test_get_lsp_args_with_logging() {
        let mut settings = test_lsp_settings();
        settings.log_enabled = true;
        settings.log_path = Some(PathBuf::from("/tmp/lsp.log"));
        let args = settings.get_lsp_args();
        assert!(args.contains(&"-V".to_string()));
        assert!(args.contains(&"--enable-log=true".to_string()));
        assert!(args.iter().any(|a| a.contains("--log-path=")));
    }

    #[test]
    fn test_lsp_server_path() {
        let settings = test_lsp_settings();
        let path = settings.lsp_server_path();
        let expected = if cfg!(windows) {
            PathBuf::from("/opt/cangjie-sdk/tools/bin/LSPServer.exe")
        } else {
            PathBuf::from("/opt/cangjie-sdk/tools/bin/LSPServer")
        };
        assert_eq!(path, expected);
    }

    #[test]
    fn test_validate_nonexistent_paths() {
        let settings = LSPSettings {
            sdk_path: PathBuf::from("/nonexistent/sdk"),
            workspace_path: PathBuf::from("/nonexistent/workspace"),
            log_enabled: false,
            log_path: None,
            init_timeout_ms: 30000,
            disable_auto_import: false,
        };
        let errors = settings.validate();
        assert!(!errors.is_empty());
        assert!(errors.iter().any(|e| e.contains("SDK path")));
    }

    #[test]
    fn test_lsp_init_options_default() {
        let options = LSPInitOptions::default();
        assert!(options.multi_module_option.is_empty());
        assert!(!options.telemetry_option);
    }

    #[test]
    fn test_lsp_init_options_serialization() {
        let options = LSPInitOptions {
            telemetry_option: false,
            ..Default::default()
        };
        let json = serde_json::to_value(&options).unwrap();
        assert_eq!(json["telemetryOption"], false);
        // clangdFileStatus and fallbackFlags should not be present
        assert!(json.get("clangdFileStatus").is_none());
        assert!(json.get("fallbackFlags").is_none());
    }

    #[test]
    fn test_envsetup_script_path() {
        let settings = test_lsp_settings();
        let path = settings.envsetup_script_path();
        if cfg!(windows) {
            assert_eq!(path, PathBuf::from("/opt/cangjie-sdk/envsetup.ps1"));
        } else {
            assert_eq!(path, PathBuf::from("/opt/cangjie-sdk/envsetup.sh"));
        }
    }

    #[test]
    fn test_validate_missing_envsetup_script() {
        let settings = LSPSettings {
            sdk_path: PathBuf::from("/nonexistent/sdk"),
            workspace_path: PathBuf::from("/nonexistent/workspace"),
            log_enabled: false,
            log_path: None,
            init_timeout_ms: 30000,
            disable_auto_import: false,
        };
        let errors = settings.validate();
        assert!(errors
            .iter()
            .any(|e| e.contains("Environment setup script")));
    }

    #[test]
    fn test_get_lsp_args_log_enabled_but_no_path() {
        let mut settings = test_lsp_settings();
        settings.log_enabled = true;
        settings.log_path = None;
        let args = settings.get_lsp_args();
        // log_enabled but no log_path → the inner `if let` doesn't match,
        // so no log flags are added at all
        assert!(!args.contains(&"--enable-log=false".to_string()));
        assert!(!args.contains(&"-V".to_string()));
        assert!(!args.contains(&"--enable-log=true".to_string()));
    }

    #[test]
    fn test_validate_with_existing_workspace() {
        // Use a directory that exists (but SDK won't)
        let settings = LSPSettings {
            sdk_path: PathBuf::from("/nonexistent/sdk"),
            workspace_path: std::env::current_dir().unwrap(),
            log_enabled: false,
            log_path: None,
            init_timeout_ms: 30000,
            disable_auto_import: false,
        };
        let errors = settings.validate();
        // SDK path error should be present, but workspace should not
        assert!(errors.iter().any(|e| e.contains("SDK path")));
        assert!(!errors.iter().any(|e| e.contains("Workspace path")));
    }

    #[test]
    fn test_lsp_init_options_full_serialization() {
        let mut mm = HashMap::new();
        mm.insert(
            "module1".to_string(),
            serde_json::json!({"path": "/some/path"}),
        );
        let options = LSPInitOptions {
            multi_module_option: mm,
            std_lib_path_option: "/std/lib".to_string(),
            target_lib: "my_target".to_string(),
            ..Default::default()
        };
        let json = serde_json::to_value(&options).unwrap();
        assert_eq!(json["stdLibPathOption"], "/std/lib");
        assert_eq!(json["targetLib"], "my_target");
        assert!(json["multiModuleOption"]["module1"].is_object());
    }

    #[test]
    fn test_build_init_options_no_cjpm_toml() {
        let tmp = tempfile::TempDir::new().unwrap();
        let settings = LSPSettings {
            sdk_path: PathBuf::from("/opt/cangjie-sdk"),
            workspace_path: tmp.path().to_path_buf(),
            log_enabled: false,
            log_path: None,
            init_timeout_ms: 30000,
            disable_auto_import: false,
        };
        let (options, require_path) = build_init_options(&settings);
        // No cjpm.toml means no modules found
        assert!(options.multi_module_option.is_empty() || options.multi_module_option.len() == 1);
        assert!(require_path.is_empty());
    }

    #[test]
    fn test_build_init_options_with_basic_cjpm_toml() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("cjpm.toml"),
            "[package]\nname = \"test-pkg\"\n",
        )
        .unwrap();
        let settings = LSPSettings {
            sdk_path: PathBuf::from("/opt/cangjie-sdk"),
            workspace_path: tmp.path().to_path_buf(),
            log_enabled: false,
            log_path: None,
            init_timeout_ms: 30000,
            disable_auto_import: false,
        };
        let (options, require_path) = build_init_options(&settings);
        // Should have at least one module option
        assert!(!options.multi_module_option.is_empty());
        assert!(require_path.is_empty());
    }

    #[test]
    fn test_build_init_options_std_lib_from_sdk() {
        let tmp = tempfile::TempDir::new().unwrap();
        let sdk_dir = tmp.path().join("sdk");
        std::fs::create_dir_all(&sdk_dir).unwrap();

        let ws_dir = tmp.path().join("workspace");
        std::fs::create_dir_all(&ws_dir).unwrap();

        let settings = LSPSettings {
            sdk_path: sdk_dir.clone(),
            workspace_path: ws_dir,
            log_enabled: false,
            log_path: None,
            init_timeout_ms: 30000,
            disable_auto_import: false,
        };
        let (options, _require_path) = build_init_options(&settings);
        // Always <sdk_path>/lib — must never be empty
        assert_eq!(
            options.std_lib_path_option,
            sdk_dir.join("lib").to_string_lossy().to_string()
        );
    }

    #[test]
    fn test_validate_all_nonexistent() {
        let settings = LSPSettings {
            sdk_path: PathBuf::from("/nonexistent/sdk"),
            workspace_path: PathBuf::from("/nonexistent/workspace"),
            log_enabled: false,
            log_path: None,
            init_timeout_ms: 30000,
            disable_auto_import: false,
        };
        let errors = settings.validate();
        // Should have errors for SDK path, LSP server, and workspace path
        assert!(
            errors.len() >= 2,
            "Expected at least 2 errors, got: {:?}",
            errors
        );
        assert!(errors.iter().any(|e| e.contains("SDK path")));
        assert!(errors.iter().any(|e| e.contains("Workspace path")));
    }

    #[test]
    fn test_validate_workspace_exists_sdk_not() {
        let tmp = tempfile::TempDir::new().unwrap();
        let settings = LSPSettings {
            sdk_path: PathBuf::from("/nonexistent/sdk"),
            workspace_path: tmp.path().to_path_buf(),
            log_enabled: false,
            log_path: None,
            init_timeout_ms: 30000,
            disable_auto_import: false,
        };
        let errors = settings.validate();
        assert!(errors.iter().any(|e| e.contains("SDK path")));
        assert!(errors.iter().any(|e| e.contains("LSP server")));
        assert!(!errors.iter().any(|e| e.contains("Workspace path")));
    }

    #[test]
    fn test_get_lsp_args_with_all_options() {
        let settings = LSPSettings {
            sdk_path: PathBuf::from("/sdk"),
            workspace_path: PathBuf::from("/ws"),
            log_enabled: true,
            log_path: Some(PathBuf::from("/var/log/lsp.log")),
            init_timeout_ms: 60000,
            disable_auto_import: true,
        };
        let args = settings.get_lsp_args();
        // Should have: "src", "--disableAutoImport", "-V", "--enable-log=true", "--log-path=..."
        assert_eq!(args[0], "src");
        assert!(args.contains(&"--disableAutoImport".to_string()));
        assert!(args.contains(&"-V".to_string()));
        assert!(args.contains(&"--enable-log=true".to_string()));
        assert!(args.iter().any(|a| a == "--log-path=/var/log/lsp.log"));
        // Should NOT contain --enable-log=false when logging is enabled
        assert!(!args.contains(&"--enable-log=false".to_string()));
    }

    #[test]
    fn test_lsp_server_path_includes_tools_bin() {
        let settings = LSPSettings {
            sdk_path: PathBuf::from("/my/custom/sdk"),
            workspace_path: PathBuf::from("/ws"),
            log_enabled: false,
            log_path: None,
            init_timeout_ms: 30000,
            disable_auto_import: false,
        };
        let path = settings.lsp_server_path();
        assert!(path.starts_with("/my/custom/sdk"));
        assert!(path.to_string_lossy().contains("tools"));
        assert!(path.to_string_lossy().contains("bin"));
        if cfg!(windows) {
            assert!(path.to_string_lossy().ends_with("LSPServer.exe"));
        } else {
            assert!(path.to_string_lossy().ends_with("LSPServer"));
        }
    }

    #[test]
    fn test_build_init_options_std_lib_never_empty() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("cjpm.toml"),
            "[package]\nname = \"test-pkg\"\n",
        )
        .unwrap();
        let settings = LSPSettings {
            sdk_path: PathBuf::from("/opt/cangjie-sdk"),
            workspace_path: tmp.path().to_path_buf(),
            log_enabled: false,
            log_path: None,
            init_timeout_ms: 30000,
            disable_auto_import: false,
        };
        let (options, _) = build_init_options(&settings);
        // Must never be empty — empty string causes IsInCjlibDir() to
        // match every path, suppressing LSP responses.
        assert!(!options.std_lib_path_option.is_empty());
        assert!(options.std_lib_path_option.contains("lib"));
    }

    #[test]
    fn test_lsp_init_options_condition_compile_serialization() {
        let mut cco = HashMap::new();
        cco.insert("feature_x".to_string(), serde_json::json!(true));
        let options = LSPInitOptions {
            condition_compile_option: cco,
            ..Default::default()
        };
        let json = serde_json::to_value(&options).unwrap();
        assert_eq!(json["conditionCompileOption"]["feature_x"], true);
    }

    #[test]
    fn test_lsp_init_options_single_condition_compile_serialization() {
        let mut scco = HashMap::new();
        scco.insert("single_feature".to_string(), serde_json::json!("value"));
        let options = LSPInitOptions {
            single_condition_compile_option: scco,
            condition_compile_paths: vec!["/path/a".to_string(), "/path/b".to_string()],
            ..Default::default()
        };
        let json = serde_json::to_value(&options).unwrap();
        assert_eq!(
            json["singleConditionCompileOption"]["single_feature"],
            "value"
        );
        assert_eq!(json["conditionCompilePaths"][0], "/path/a");
        assert_eq!(json["conditionCompilePaths"][1], "/path/b");
    }

    #[test]
    fn test_lsp_init_options_modules_home_and_extension_path() {
        let options = LSPInitOptions {
            modules_home_option: "/home/modules".to_string(),
            extension_path: "/ext/path".to_string(),
            ..Default::default()
        };
        let json = serde_json::to_value(&options).unwrap();
        assert_eq!(json["modulesHomeOption"], "/home/modules");
        assert_eq!(json["extensionPath"], "/ext/path");
    }
}
