use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::lsp::dependency::DependencyResolver;
use crate::lsp::utils::load_cjpm_toml;
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
    #[serde(default)]
    pub clangd_file_status: bool,
    #[serde(default)]
    pub fallback_flags: Vec<String>,
}

// -- Platform environment ----------------------------------------------------

pub fn get_platform_env(sdk_path: &Path) -> HashMap<String, String> {
    let mut env: HashMap<String, String> = std::env::vars().collect();

    if cfg!(windows) {
        get_windows_env(sdk_path, &mut env);
    } else if cfg!(target_os = "macos") {
        get_darwin_env(sdk_path, &mut env);
    } else {
        get_linux_env(sdk_path, &mut env);
    }

    env
}

fn get_linux_env(sdk_path: &Path, env: &mut HashMap<String, String>) {
    let arch = std::process::Command::new("arch")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "x86_64".to_string());

    let ld_lib = sdk_path
        .join("lib")
        .join(format!("linux_{arch}_llvm"))
        .to_string_lossy()
        .to_string();
    let bin_path = sdk_path
        .join("tools")
        .join("bin")
        .to_string_lossy()
        .to_string();

    let existing_ld = env.get("LD_LIBRARY_PATH").cloned().unwrap_or_default();
    env.insert(
        "LD_LIBRARY_PATH".to_string(),
        if existing_ld.is_empty() {
            ld_lib
        } else {
            format!("{ld_lib}:{existing_ld}")
        },
    );

    let existing_path = env.get("PATH").cloned().unwrap_or_default();
    env.insert(
        "PATH".to_string(),
        if existing_path.is_empty() {
            bin_path
        } else {
            format!("{bin_path}:{existing_path}")
        },
    );
}

fn get_darwin_env(sdk_path: &Path, env: &mut HashMap<String, String>) {
    let arch = if std::env::consts::ARCH == "aarch64" {
        "aarch64"
    } else {
        "x86_64"
    };

    let dyld_lib = sdk_path
        .join("lib")
        .join(format!("darwin_{arch}_llvm"))
        .to_string_lossy()
        .to_string();
    let bin_path = sdk_path
        .join("tools")
        .join("bin")
        .to_string_lossy()
        .to_string();

    let existing_dyld = env.get("DYLD_LIBRARY_PATH").cloned().unwrap_or_default();
    env.insert(
        "DYLD_LIBRARY_PATH".to_string(),
        if existing_dyld.is_empty() {
            dyld_lib
        } else {
            format!("{dyld_lib}:{existing_dyld}")
        },
    );

    let existing_path = env.get("PATH").cloned().unwrap_or_default();
    env.insert(
        "PATH".to_string(),
        if existing_path.is_empty() {
            bin_path
        } else {
            format!("{bin_path}:{existing_path}")
        },
    );
}

fn get_windows_env(sdk_path: &Path, env: &mut HashMap<String, String>) {
    let runtime_lib = sdk_path
        .join("runtime")
        .join("lib")
        .join("windows_x86_64_llvm")
        .to_string_lossy()
        .to_string();
    let bin = sdk_path.join("bin").to_string_lossy().to_string();
    let tools_bin = sdk_path
        .join("tools")
        .join("bin")
        .to_string_lossy()
        .to_string();
    let existing_path = env.get("PATH").cloned().unwrap_or_default();

    let paths: Vec<&str> = [
        runtime_lib.as_str(),
        bin.as_str(),
        tools_bin.as_str(),
        existing_path.as_str(),
    ]
    .into_iter()
    .filter(|p| !p.is_empty())
    .collect();

    env.insert("PATH".to_string(), paths.join(";"));
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

    let cjpm_toml_path = settings.workspace_path.join("cjpm.toml");
    let std_lib_path = if let Some(cjpm) = load_cjpm_toml(&cjpm_toml_path) {
        if let Some(pkg) = &cjpm.package {
            if !pkg.target_dir.is_empty() {
                pkg.target_dir.clone()
            } else {
                String::new()
            }
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    let options = LSPInitOptions {
        multi_module_option,
        std_lib_path_option: std_lib_path,
        clangd_file_status: true,
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
        assert!(!options.clangd_file_status);
    }

    #[test]
    fn test_lsp_init_options_serialization() {
        let options = LSPInitOptions {
            clangd_file_status: true,
            telemetry_option: false,
            ..Default::default()
        };
        let json = serde_json::to_value(&options).unwrap();
        assert_eq!(json["clangdFileStatus"], true);
        assert_eq!(json["telemetryOption"], false);
    }

    #[test]
    fn test_get_platform_env_sets_path() {
        let sdk_path = PathBuf::from("/opt/cangjie-sdk");
        let env = get_platform_env(&sdk_path);
        // Regardless of platform, PATH should be set
        assert!(env.contains_key("PATH"));
        let path_val = &env["PATH"];
        // PATH should include SDK's tools/bin
        assert!(
            path_val.contains("tools") && path_val.contains("bin"),
            "PATH should include SDK tools/bin directory, got: {}",
            path_val
        );
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
            clangd_file_status: true,
            fallback_flags: vec!["-Wall".to_string()],
            ..Default::default()
        };
        let json = serde_json::to_value(&options).unwrap();
        assert_eq!(json["stdLibPathOption"], "/std/lib");
        assert_eq!(json["targetLib"], "my_target");
        assert!(json["multiModuleOption"]["module1"].is_object());
        assert_eq!(json["fallbackFlags"][0], "-Wall");
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
    fn test_build_init_options_with_target_dir() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("cjpm.toml"),
            "[package]\nname = \"test-pkg\"\ntarget-dir = \"/custom/target\"\n",
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
        let (options, _require_path) = build_init_options(&settings);
        assert_eq!(options.std_lib_path_option, "/custom/target");
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
    fn test_build_init_options_cjpm_toml_package_no_target_dir() {
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
        // No target-dir means std_lib_path_option should be empty
        assert!(options.std_lib_path_option.is_empty());
    }

    #[test]
    fn test_build_init_options_cjpm_toml_empty_target_dir() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("cjpm.toml"),
            "[package]\nname = \"test-pkg\"\ntarget-dir = \"\"\n",
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
        // Empty target-dir means std_lib_path_option should be empty
        assert!(options.std_lib_path_option.is_empty());
    }

    #[test]
    fn test_build_init_options_cjpm_toml_no_package_section() {
        let tmp = tempfile::TempDir::new().unwrap();
        // TOML without [package] section - just dependencies
        std::fs::write(tmp.path().join("cjpm.toml"), "[dependencies]\n").unwrap();
        let settings = LSPSettings {
            sdk_path: PathBuf::from("/opt/cangjie-sdk"),
            workspace_path: tmp.path().to_path_buf(),
            log_enabled: false,
            log_path: None,
            init_timeout_ms: 30000,
            disable_auto_import: false,
        };
        let (options, _) = build_init_options(&settings);
        // No package section -> std_lib_path_option should be empty
        assert!(options.std_lib_path_option.is_empty());
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

    #[test]
    fn test_get_platform_env_contains_system_vars() {
        let sdk_path = PathBuf::from("/opt/cangjie-sdk");
        let env = get_platform_env(&sdk_path);
        // Should contain at least PATH (on any platform)
        assert!(env.contains_key("PATH"));
        // The env should also contain regular system environment variables
        // (since it starts from std::env::vars())
        assert!(
            env.len() > 1,
            "Platform env should contain multiple variables"
        );
    }

    #[test]
    fn test_build_init_options_sets_clangd_file_status() {
        let tmp = tempfile::TempDir::new().unwrap();
        let settings = LSPSettings {
            sdk_path: PathBuf::from("/opt/cangjie-sdk"),
            workspace_path: tmp.path().to_path_buf(),
            log_enabled: false,
            log_path: None,
            init_timeout_ms: 30000,
            disable_auto_import: false,
        };
        let (options, _) = build_init_options(&settings);
        assert!(
            options.clangd_file_status,
            "clangd_file_status should be set to true"
        );
    }
}
