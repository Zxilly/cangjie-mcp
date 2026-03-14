use std::path::PathBuf;

fn runtime_dir() -> PathBuf {
    #[cfg(unix)]
    {
        if let Ok(dir) = std::env::var("XDG_RUNTIME_DIR") {
            return PathBuf::from(dir).join("cangjie");
        }
        let uid = unsafe { libc::getuid() };
        PathBuf::from(format!("/tmp/cangjie-{uid}"))
    }

    #[cfg(windows)]
    {
        dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("cangjie")
    }
}

pub fn pid_file() -> PathBuf {
    runtime_dir().join("daemon.pid")
}

pub fn log_file() -> PathBuf {
    runtime_dir().join("daemon.log")
}

#[cfg(unix)]
pub fn socket_path() -> PathBuf {
    runtime_dir().join("daemon.sock")
}

#[cfg(windows)]
pub fn pipe_name() -> String {
    r"\\.\pipe\cangjie-daemon".to_string()
}

pub fn ensure_runtime_dir() -> std::io::Result<()> {
    std::fs::create_dir_all(runtime_dir())
}
