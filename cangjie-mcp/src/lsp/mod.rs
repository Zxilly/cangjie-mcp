pub mod client;
pub mod config;
pub mod dependency;
pub mod tools;
pub mod utils;

use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::RwLock;
use tracing::{error, info};

use crate::lsp::client::CangjieClient;
use crate::lsp::config::{build_init_options, get_platform_env, LSPSettings};
use crate::lsp::utils::get_path_separator;

static LSP_CLIENT: once_cell::sync::Lazy<Arc<RwLock<Option<CangjieClient>>>> =
    once_cell::sync::Lazy::new(|| Arc::new(RwLock::new(None)));

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
    // Can't do async check here, so we just check if CANGJIE_HOME is set
    std::env::var("CANGJIE_HOME").is_ok()
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
    let sdk_path = std::env::var("CANGJIE_HOME").ok().map(PathBuf::from)?;

    let workspace = workspace_path
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

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
    use std::sync::Mutex;

    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    #[test]
    fn test_is_available_checks_env() {
        let _lock = ENV_MUTEX.lock().unwrap();
        let original = std::env::var("CANGJIE_HOME").ok();

        std::env::set_var("CANGJIE_HOME", "/tmp/fake-cangjie-sdk");
        assert!(is_available());

        std::env::remove_var("CANGJIE_HOME");
        assert!(!is_available());

        if let Some(val) = original {
            std::env::set_var("CANGJIE_HOME", val);
        }
    }

    #[test]
    fn test_detect_settings_no_cangjie_home() {
        let _lock = ENV_MUTEX.lock().unwrap();
        let original = std::env::var("CANGJIE_HOME").ok();

        std::env::remove_var("CANGJIE_HOME");
        let result = detect_settings(Some(PathBuf::from("/tmp/test")));
        assert!(result.is_none());

        if let Some(val) = original {
            std::env::set_var("CANGJIE_HOME", val);
        }
    }

    #[test]
    fn test_detect_settings_with_cangjie_home() {
        let _lock = ENV_MUTEX.lock().unwrap();
        let original = std::env::var("CANGJIE_HOME").ok();

        std::env::set_var("CANGJIE_HOME", "/tmp/fake-cangjie-sdk");
        let result = detect_settings(Some(PathBuf::from("/tmp/workspace")));
        assert!(result.is_some());

        let settings = result.unwrap();
        assert_eq!(settings.sdk_path, PathBuf::from("/tmp/fake-cangjie-sdk"));
        assert_eq!(settings.workspace_path, PathBuf::from("/tmp/workspace"));
        assert!(!settings.log_enabled);
        assert!(settings.log_path.is_none());
        assert_eq!(settings.init_timeout_ms, 45000);
        assert!(settings.disable_auto_import);

        if let Some(val) = original {
            std::env::set_var("CANGJIE_HOME", val);
        } else {
            std::env::remove_var("CANGJIE_HOME");
        }
    }

    #[test]
    fn test_detect_settings_with_workspace() {
        let _lock = ENV_MUTEX.lock().unwrap();
        let original = std::env::var("CANGJIE_HOME").ok();

        std::env::set_var("CANGJIE_HOME", "/tmp/fake-cangjie-sdk");
        let workspace = PathBuf::from("/my/custom/workspace");
        let result = detect_settings(Some(workspace.clone()));
        assert!(result.is_some());
        assert_eq!(result.unwrap().workspace_path, workspace);

        if let Some(val) = original {
            std::env::set_var("CANGJIE_HOME", val);
        } else {
            std::env::remove_var("CANGJIE_HOME");
        }
    }

    #[test]
    fn test_detect_settings_default_workspace() {
        let _lock = ENV_MUTEX.lock().unwrap();
        let original = std::env::var("CANGJIE_HOME").ok();

        std::env::set_var("CANGJIE_HOME", "/tmp/fake-cangjie-sdk");
        let result = detect_settings(None);
        assert!(result.is_some());

        let settings = result.unwrap();
        let expected = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        assert_eq!(settings.workspace_path, expected);

        if let Some(val) = original {
            std::env::set_var("CANGJIE_HOME", val);
        } else {
            std::env::remove_var("CANGJIE_HOME");
        }
    }
}
