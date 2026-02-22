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
}
