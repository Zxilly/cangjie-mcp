pub mod client;
pub mod config;
pub mod dependency;
pub mod tools;
pub mod utils;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::sync::RwLock;
use tracing::{error, info};

use crate::lsp::client::CangjieClient;
use crate::lsp::config::{build_init_options, get_platform_env, LSPSettings};
use crate::lsp::utils::get_path_separator;

static LSP_CLIENT: once_cell::sync::Lazy<Arc<RwLock<Option<CangjieClient>>>> =
    once_cell::sync::Lazy::new(|| Arc::new(RwLock::new(None)));

fn detect_cangjie_home_from_vscode_settings(workspace: &Path) -> Option<PathBuf> {
    let settings_path = workspace.join(".vscode").join("settings.json");
    let content = std::fs::read_to_string(settings_path).ok()?;
    let root: serde_json::Value = serde_json::from_str(&content).ok()?;
    let root_obj = root.as_object()?;

    for (key, value) in root_obj {
        if !key.starts_with("terminal.integrated.env") {
            continue;
        }

        let env_obj = match value.as_object() {
            Some(env_obj) => env_obj,
            None => continue,
        };

        let sdk_path = match env_obj.get("CANGJIE_HOME").and_then(|v| v.as_str()) {
            Some(sdk_path) if !sdk_path.trim().is_empty() => sdk_path,
            _ => continue,
        };

        return Some(PathBuf::from(sdk_path));
    }

    None
}

fn detect_cangjie_home(workspace: &Path) -> Option<PathBuf> {
    std::env::var("CANGJIE_HOME")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .or_else(|| detect_cangjie_home_from_vscode_settings(workspace))
}

/// Initialize the global LSP client.
pub async fn init(settings: LSPSettings) -> bool {
    let mut env = get_platform_env(&settings.sdk_path);
    let (init_options, require_path) = build_init_options(&settings);

    if !require_path.is_empty() {
        let sep = get_path_separator();
        let existing = env.get("PATH").cloned().unwrap_or_default();
        env.insert(
            "PATH".to_string(),
            if existing.is_empty() {
                require_path.clone()
            } else {
                format!("{require_path}{sep}{existing}")
            },
        );
    }

    match CangjieClient::start(&settings, &init_options, &env).await {
        Ok(client) => {
            *LSP_CLIENT.write().await = Some(client);
            info!("LSP client initialized successfully");
            true
        }
        Err(e) => {
            error!("Failed to initialize LSP client: {}", e);
            false
        }
    }
}

/// Shutdown the global LSP client.
pub async fn shutdown() {
    let mut guard = LSP_CLIENT.write().await;
    if let Some(ref client) = *guard {
        let _ = client.shutdown().await;
    }
    *guard = None;
    info!("LSP client shutdown complete");
}

/// Check if the LSP client is available.
pub fn is_available() -> bool {
    let workspace = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    detect_cangjie_home(&workspace).is_some()
}

/// Get a reference to the global LSP client (read lock).
pub async fn get_client() -> Option<tokio::sync::RwLockReadGuard<'static, Option<CangjieClient>>> {
    let guard = LSP_CLIENT.read().await;
    if guard.is_some() {
        Some(guard)
    } else {
        None
    }
}

/// Try to auto-detect and create LSP settings.
pub fn detect_settings(workspace_path: Option<PathBuf>) -> Option<LSPSettings> {
    let workspace = workspace_path
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let sdk_path = detect_cangjie_home(&workspace)?;

    Some(LSPSettings {
        sdk_path,
        workspace_path: workspace,
        log_enabled: false,
        log_path: None,
        init_timeout_ms: 45000,
        disable_auto_import: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write_workspace_settings(workspace: &Path, content: &str) {
        let vscode_dir = workspace.join(".vscode");
        std::fs::create_dir_all(&vscode_dir).unwrap();
        std::fs::write(vscode_dir.join("settings.json"), content).unwrap();
    }

    #[test]
    fn test_is_available_checks_env() {
        temp_env::with_var("CANGJIE_HOME", Some("/tmp/fake-cangjie-sdk"), || {
            assert!(is_available());
        });
        temp_env::with_var("CANGJIE_HOME", None::<&str>, || {
            assert!(!is_available());
        });
    }

    #[test]
    fn test_detect_settings_no_cangjie_home() {
        temp_env::with_var("CANGJIE_HOME", None::<&str>, || {
            let result = detect_settings(Some(PathBuf::from("/tmp/test")));
            assert!(result.is_none());
        });
    }

    #[test]
    fn test_detect_settings_with_cangjie_home() {
        temp_env::with_var("CANGJIE_HOME", Some("/tmp/fake-cangjie-sdk"), || {
            let result = detect_settings(Some(PathBuf::from("/tmp/workspace")));
            assert!(result.is_some());

            let settings = result.unwrap();
            assert_eq!(settings.sdk_path, PathBuf::from("/tmp/fake-cangjie-sdk"));
            assert_eq!(settings.workspace_path, PathBuf::from("/tmp/workspace"));
            assert!(!settings.log_enabled);
            assert!(settings.log_path.is_none());
            assert_eq!(settings.init_timeout_ms, 45000);
            assert!(settings.disable_auto_import);
        });
    }

    #[test]
    fn test_detect_settings_with_workspace() {
        temp_env::with_var("CANGJIE_HOME", Some("/tmp/fake-cangjie-sdk"), || {
            let workspace = PathBuf::from("/my/custom/workspace");
            let result = detect_settings(Some(workspace.clone()));
            assert!(result.is_some());
            assert_eq!(result.unwrap().workspace_path, workspace);
        });
    }

    #[test]
    fn test_detect_settings_default_workspace() {
        temp_env::with_var("CANGJIE_HOME", Some("/tmp/fake-cangjie-sdk"), || {
            let result = detect_settings(None);
            assert!(result.is_some());

            let settings = result.unwrap();
            let expected = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            assert_eq!(settings.workspace_path, expected);
        });
    }

    #[test]
    fn test_detect_settings_falls_back_to_vscode_terminal_env() {
        let temp_dir = TempDir::new().unwrap();
        write_workspace_settings(
            temp_dir.path(),
            r#"{
  "terminal.integrated.env.windows": {
    "CANGJIE_HOME": "/from/vscode/settings"
  }
}"#,
        );

        temp_env::with_var("CANGJIE_HOME", None::<&str>, || {
            let result = detect_settings(Some(temp_dir.path().to_path_buf()));
            assert!(result.is_some());
            assert_eq!(
                result.unwrap().sdk_path,
                PathBuf::from("/from/vscode/settings")
            );
        });
    }

    #[test]
    fn test_detect_settings_env_takes_precedence_over_vscode_fallback() {
        let temp_dir = TempDir::new().unwrap();
        write_workspace_settings(
            temp_dir.path(),
            r#"{
  "terminal.integrated.env.windows": {
    "CANGJIE_HOME": "/from/vscode/settings"
  }
}"#,
        );

        temp_env::with_var("CANGJIE_HOME", Some("/from/env"), || {
            let result = detect_settings(Some(temp_dir.path().to_path_buf()));
            assert!(result.is_some());
            assert_eq!(result.unwrap().sdk_path, PathBuf::from("/from/env"));
        });
    }
}
