use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;
use tracing::info;

use cangjie_lsp::client::CangjieClient;

struct LspEntry {
    client: Arc<CangjieClient>,
    last_activity: AtomicU64, // epoch seconds
}

impl LspEntry {
    fn new(client: Arc<CangjieClient>) -> Self {
        Self {
            client,
            last_activity: AtomicU64::new(epoch_secs()),
        }
    }

    fn touch(&self) {
        self.last_activity.store(epoch_secs(), Ordering::Relaxed);
    }

    fn idle_secs(&self) -> u64 {
        epoch_secs() - self.last_activity.load(Ordering::Relaxed)
    }
}

/// Manages multiple LSP client instances keyed by workspace directory.
pub struct LspPool {
    entries: RwLock<HashMap<PathBuf, LspEntry>>,
    idle_timeout: Duration,
}

impl LspPool {
    pub fn new(idle_timeout: Duration) -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
            idle_timeout,
        }
    }

    /// Get an existing healthy client or create a new one for the workspace.
    pub async fn get_or_create(&self, workspace_path: &Path) -> Result<Arc<CangjieClient>, String> {
        // Fast path: read lock, check existing healthy entry
        {
            let entries = self.entries.read().await;
            if let Some(entry) = entries.get(workspace_path) {
                if entry.client.is_running() {
                    entry.touch();
                    return Ok(entry.client.clone());
                }
            }
        }

        // Slow path: write lock, create or replace
        let mut entries = self.entries.write().await;

        // Double-check after acquiring write lock
        if let Some(entry) = entries.get(workspace_path) {
            if entry.client.is_running() {
                entry.touch();
                return Ok(entry.client.clone());
            }
            info!(
                "LSP client for {} is no longer running, replacing",
                workspace_path.display()
            );
            let _ = entry.client.shutdown().await;
            entries.remove(workspace_path);
        }

        // Create new client
        let settings = cangjie_lsp::detect_settings(Some(workspace_path.to_path_buf()))
            .ok_or_else(|| {
                format!(
                    "Cannot create LSP client for {}: CANGJIE_HOME not configured",
                    workspace_path.display()
                )
            })?;
        let client = Arc::new(cangjie_lsp::start_client(&settings).await?);
        entries.insert(workspace_path.to_path_buf(), LspEntry::new(client.clone()));
        info!(
            "LSP pool: created new client for {} (total: {})",
            workspace_path.display(),
            entries.len()
        );
        Ok(client)
    }

    /// Shut down and remove idle entries.
    pub async fn evict_idle(&self) {
        let timeout_secs = self.idle_timeout.as_secs();

        // Collect entries to evict under read lock (no await on shutdown)
        let to_evict: Vec<PathBuf> = {
            let entries = self.entries.read().await;
            if entries.is_empty() {
                return;
            }
            entries
                .iter()
                .filter(|(_, e)| e.idle_secs() >= timeout_secs || !e.client.is_running())
                .map(|(path, _)| path.clone())
                .collect()
        };

        if to_evict.is_empty() {
            return;
        }

        // Remove under write lock, then shutdown outside the lock
        let evicted: Vec<(PathBuf, Arc<CangjieClient>)> = {
            let mut entries = self.entries.write().await;
            to_evict
                .into_iter()
                .filter_map(|path| entries.remove(&path).map(|e| (path, e.client)))
                .collect()
        };

        for (path, client) in &evicted {
            if client.is_running() {
                info!("LSP pool: evicting idle client for {}", path.display());
                let _ = client.shutdown().await;
            } else {
                info!("LSP pool: removing dead client for {}", path.display());
            }
        }

        info!("LSP pool: evicted {} entries", evicted.len());
    }

    /// Shut down all entries (for daemon shutdown).
    pub async fn shutdown_all(&self) {
        let all: Vec<(PathBuf, Arc<CangjieClient>)> = {
            let mut entries = self.entries.write().await;
            entries.drain().map(|(path, e)| (path, e.client)).collect()
        };
        for (path, client) in &all {
            info!("LSP pool: shutting down client for {}", path.display());
            let _ = client.shutdown().await;
        }
    }
}

fn epoch_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
