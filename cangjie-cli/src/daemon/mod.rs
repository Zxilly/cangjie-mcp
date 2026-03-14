pub mod ipc;
pub mod paths;
pub mod server;

use std::time::Duration;

use anyhow::{Context, Result};
use serde_json::json;
use tracing::info;

use self::ipc::ipc_connect;

/// Ensure the daemon is running. Returns Ok(()) if a connection can be made.
pub async fn ensure_running(timeout_minutes: u64) -> Result<()> {
    // Try connecting first
    if ipc_connect().await.is_ok() {
        return Ok(());
    }

    // Need to spawn daemon
    spawn_daemon(timeout_minutes)?;

    // Wait for socket to become available
    for i in 0..100 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if ipc_connect().await.is_ok() {
            info!("Daemon ready after {}ms", (i + 1) * 100);
            return Ok(());
        }
    }

    anyhow::bail!("Daemon failed to start within 10 seconds")
}

fn spawn_daemon(timeout_minutes: u64) -> Result<()> {
    let exe = std::env::current_exe().context("failed to get current executable path")?;

    let mut cmd = std::process::Command::new(exe);
    cmd.args(["mcp", "serve"]);
    cmd.arg("--daemon-timeout");
    cmd.arg(timeout_minutes.to_string());

    // Detach from terminal
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
        cmd.stdin(std::process::Stdio::null());
        cmd.stdout(std::process::Stdio::null());
        cmd.stderr(std::process::Stdio::null());
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        const DETACHED_PROCESS: u32 = 0x00000008;
        cmd.creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS);
        cmd.stdin(std::process::Stdio::null());
        cmd.stdout(std::process::Stdio::null());
        cmd.stderr(std::process::Stdio::null());
    }

    cmd.spawn().context("failed to spawn daemon process")?;
    info!("Spawned daemon process");
    Ok(())
}

pub fn stop_daemon() -> Result<()> {
    let pid_path = paths::pid_file();
    let pid_str = match std::fs::read_to_string(&pid_path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            println!(
                "{}",
                json!({"status": "stopped", "message": "No daemon running (no PID file)"})
            );
            return Ok(());
        }
        Err(e) => return Err(e).context("failed to read PID file"),
    };
    let pid: u32 = pid_str.trim().parse().context("invalid PID in file")?;

    kill_process(pid)?;

    // Clean up files
    let _ = std::fs::remove_file(&pid_path);
    #[cfg(unix)]
    let _ = std::fs::remove_file(paths::socket_path());

    println!(
        "{}",
        json!({"status": "stopped", "pid": pid, "message": "Daemon stopped"})
    );
    Ok(())
}

pub fn daemon_status() -> Result<()> {
    let pid_path = paths::pid_file();
    let pid_str = match std::fs::read_to_string(&pid_path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            println!("{}", json!({"status": "stopped"}));
            return Ok(());
        }
        Err(e) => return Err(e).context("failed to read PID file"),
    };
    let pid: u32 = pid_str.trim().parse().context("invalid PID")?;

    let alive = is_process_alive(pid);
    if alive {
        println!("{}", json!({"status": "running", "pid": pid}));
    } else {
        // Stale PID file
        let _ = std::fs::remove_file(&pid_path);
        println!(
            "{}",
            json!({"status": "stopped", "message": "Stale PID file cleaned up"})
        );
    }
    Ok(())
}

pub fn daemon_logs(tail: usize, follow: bool) -> Result<()> {
    let log_path = paths::log_file();

    if follow {
        #[cfg(unix)]
        {
            let status = std::process::Command::new("tail")
                .args(["-n", &tail.to_string(), "-f"])
                .arg(&log_path)
                .status()?;
            std::process::exit(status.code().unwrap_or(1));
        }
        #[cfg(windows)]
        {
            let status = std::process::Command::new("powershell")
                .args([
                    "-Command",
                    &format!(
                        "Get-Content -Path '{}' -Tail {} -Wait",
                        log_path.display(),
                        tail
                    ),
                ])
                .status()?;
            std::process::exit(status.code().unwrap_or(1));
        }
    } else {
        use std::io::{BufRead, Seek, SeekFrom};

        let file = match std::fs::File::open(&log_path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                eprintln!("No log file found at {}", log_path.display());
                return Ok(());
            }
            Err(e) => return Err(e).context("failed to open log file"),
        };
        let file_len = file.metadata()?.len();
        let mut reader = std::io::BufReader::new(file);

        // Estimate ~200 bytes per log line, with 2x safety margin
        const BYTES_PER_LINE: u64 = 400;
        let estimated_seek = file_len.saturating_sub(BYTES_PER_LINE * tail as u64);
        if estimated_seek > 0 {
            reader.seek(SeekFrom::Start(estimated_seek))?;
            // Skip partial first line after seeking mid-file
            let mut discard = String::new();
            reader.read_line(&mut discard)?;
        }

        let remaining: Vec<String> = reader.lines().collect::<Result<_, _>>()?;
        let start = remaining.len().saturating_sub(tail);
        for line in &remaining[start..] {
            println!("{line}");
        }
    }

    Ok(())
}

fn kill_process(pid: u32) -> Result<()> {
    #[cfg(unix)]
    {
        unsafe {
            if libc::kill(pid as i32, libc::SIGTERM) != 0 {
                anyhow::bail!("Failed to send SIGTERM to PID {pid}");
            }
        }
        Ok(())
    }

    #[cfg(windows)]
    {
        let status = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/F"])
            .output()
            .context("failed to run taskkill")?;
        if !status.status.success() {
            anyhow::bail!(
                "taskkill failed: {}",
                String::from_utf8_lossy(&status.stderr)
            );
        }
        Ok(())
    }
}

fn is_process_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        unsafe { libc::kill(pid as i32, 0) == 0 }
    }

    #[cfg(windows)]
    {
        std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .output()
            .map(|o| {
                let out = String::from_utf8_lossy(&o.stdout);
                out.contains(&pid.to_string())
            })
            .unwrap_or(false)
    }
}
