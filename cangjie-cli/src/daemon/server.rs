use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use rmcp::ServiceExt;
use tracing::{error, info};

use cangjie_core::config::Settings;
use cangjie_server::CangjieServer;

use super::ipc::IpcListener;
use super::paths;

pub async fn run_daemon(settings: Settings, timeout_minutes: u64) -> Result<()> {
    // Write PID file
    paths::ensure_runtime_dir()?;
    let pid = std::process::id();
    std::fs::write(paths::pid_file(), pid.to_string())?;
    info!("Daemon started (PID={pid}, timeout={timeout_minutes}min)");

    // Create and initialize the MCP server
    let server = CangjieServer::new(settings);
    let server_init = server.clone();
    tokio::spawn(async move {
        if let Err(e) = server_init.initialize().await {
            error!("Failed to initialize server: {e}");
        }
    });

    // Bind IPC listener
    let mut listener = IpcListener::bind()?;
    info!("Daemon listening for connections");

    // Activity tracking for idle timeout
    let last_activity = Arc::new(AtomicU64::new(current_epoch_secs()));
    let timeout_secs = timeout_minutes * 60;

    // Idle timeout watchdog
    let activity = last_activity.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            let elapsed = current_epoch_secs() - activity.load(Ordering::Relaxed);
            if elapsed >= timeout_secs {
                info!("Daemon idle for {elapsed}s, shutting down");
                let _ = std::fs::remove_file(paths::pid_file());
                #[cfg(unix)]
                let _ = std::fs::remove_file(paths::socket_path());
                std::process::exit(0);
            }
        }
    });

    // Accept loop
    loop {
        match listener.accept().await {
            Ok(stream) => {
                last_activity.store(current_epoch_secs(), Ordering::Relaxed);
                let srv = server.clone();
                tokio::spawn(async move {
                    match srv.serve(stream).await {
                        Ok(service) => {
                            let _ = service.waiting().await;
                        }
                        Err(e) => {
                            error!("Failed to serve client: {e}");
                        }
                    }
                });
            }
            Err(e) => {
                error!("Accept error: {e}");
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }
}

fn current_epoch_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
