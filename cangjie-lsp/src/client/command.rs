use std::collections::HashMap;

use anyhow::{Context, Result};
use tokio::process::Command;
use tracing::info;

use crate::config::LSPSettings;

// -- Shell wrapper -----------------------------------------------------------

pub(super) fn build_shell_command(settings: &LSPSettings, require_path: &str) -> Result<Command> {
    if cfg!(windows) {
        build_windows_command(settings, require_path)
    } else {
        build_unix_command(settings, require_path)
    }
}

fn build_unix_command(settings: &LSPSettings, require_path: &str) -> Result<Command> {
    let sdk_path = settings.sdk_path.to_string_lossy();
    let envsetup = settings.envsetup_script_path();
    let envsetup_str = envsetup.to_string_lossy();
    let exe = settings.lsp_server_path();
    let exe_str = exe.to_string_lossy();
    let args = settings.get_lsp_args();

    let q_sdk = shlex::try_quote(&sdk_path)
        .map_err(|e| anyhow::anyhow!("Failed to quote SDK path: {e}"))?;
    let q_envsetup = shlex::try_quote(&envsetup_str)
        .map_err(|e| anyhow::anyhow!("Failed to quote envsetup path: {e}"))?;
    let q_exe = shlex::try_quote(&exe_str)
        .map_err(|e| anyhow::anyhow!("Failed to quote LSP server path: {e}"))?;

    let mut script = format!("export CANGJIE_HOME={q_sdk} && source {q_envsetup}",);

    if !require_path.is_empty() {
        let q_rpath = shlex::try_quote(require_path)
            .map_err(|e| anyhow::anyhow!("Failed to quote require_path: {e}"))?;
        script.push_str(&format!(" && export PATH={q_rpath}:\"$PATH\""));
    }

    script.push_str(&format!(" && exec {q_exe}"));
    for arg in &args {
        let q_arg =
            shlex::try_quote(arg).map_err(|e| anyhow::anyhow!("Failed to quote arg: {e}"))?;
        script.push_str(&format!(" {q_arg}"));
    }

    let mut cmd = Command::new("bash");
    cmd.arg("-c").arg(&script);
    Ok(cmd)
}

fn escape_powershell(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// Captures the full environment (inherited + SDK modifications) after sourcing
/// envsetup.ps1 in a separate PowerShell process, for `env_clear() + envs()` on
/// the LSP server Command.
fn capture_envsetup_env(settings: &LSPSettings) -> Result<HashMap<String, String>> {
    let sdk_path = settings.sdk_path.to_string_lossy();
    let envsetup = settings.envsetup_script_path();
    let envsetup_str = envsetup.to_string_lossy();

    let script = format!(
        "$env:CANGJIE_HOME = {}; . {} | Out-Null; \
         Get-ChildItem env: | ForEach-Object {{ \"$($_.Name)=$($_.Value)\" }}",
        escape_powershell(&sdk_path),
        escape_powershell(&envsetup_str),
    );

    info!("Capturing environment from envsetup.ps1");
    let output = std::process::Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command"])
        .arg(&script)
        .output()
        .context("Failed to run PowerShell to capture envsetup environment")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!(
            "PowerShell envsetup failed (exit {}): {}",
            output.status,
            stderr.trim()
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut env = HashMap::new();
    for line in stdout.lines() {
        if let Some((key, value)) = line.split_once('=') {
            if !key.is_empty() {
                env.insert(key.to_string(), value.to_string());
            }
        }
    }

    info!("Captured {} environment variables from envsetup", env.len());
    Ok(env)
}

/// Spawns LSPServer.exe directly (no PowerShell wrapper for stdio) using
/// environment variables captured from envsetup.ps1.
///
/// Previous approach used PowerShell as a wrapper process, but PowerShell
/// intercepts native command stdout through its pipeline, which blocked the
/// LSP server's JSON-RPC output (notably `SendMsg` in the ArkAST worker).
fn build_windows_command(settings: &LSPSettings, require_path: &str) -> Result<Command> {
    let env = capture_envsetup_env(settings)?;
    Ok(build_windows_direct_command(settings, require_path, env))
}

/// Builds a Command that runs LSPServer.exe directly with the given environment.
fn build_windows_direct_command(
    settings: &LSPSettings,
    require_path: &str,
    mut env: HashMap<String, String>,
) -> Command {
    if !require_path.is_empty() {
        let path_key = env
            .keys()
            .find(|k| k.eq_ignore_ascii_case("PATH"))
            .cloned()
            .unwrap_or_else(|| "Path".to_string());
        let current_path = env.get(&path_key).cloned().unwrap_or_default();
        env.insert(path_key, format!("{};{}", require_path, current_path));
    }

    let exe = settings.lsp_server_path();
    let args = settings.get_lsp_args();

    let mut cmd = Command::new(&exe);
    cmd.args(&args);
    cmd.env_clear();
    cmd.envs(&env);
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn test_settings(sdk: &str) -> LSPSettings {
        LSPSettings {
            sdk_path: PathBuf::from(sdk),
            workspace_path: PathBuf::from("/tmp/workspace"),
            log_enabled: false,
            log_path: None,
            init_timeout_ms: 30000,
            disable_auto_import: false,
        }
    }

    #[test]
    fn test_escape_powershell_no_quotes() {
        assert_eq!(escape_powershell("hello"), "'hello'");
    }

    #[test]
    fn test_escape_powershell_with_single_quotes() {
        assert_eq!(escape_powershell("it's"), "'it''s'");
    }

    #[test]
    fn test_escape_powershell_with_spaces() {
        assert_eq!(
            escape_powershell("C:\\Program Files\\SDK"),
            "'C:\\Program Files\\SDK'"
        );
    }

    #[test]
    fn test_escape_powershell_multiple_quotes() {
        assert_eq!(escape_powershell("a'b'c"), "'a''b''c'");
    }

    #[cfg(not(windows))]
    mod unix_tests {
        use super::*;

        #[test]
        fn test_build_unix_command_basic() {
            let settings = test_settings("/opt/cangjie-sdk");
            let cmd = build_unix_command(&settings, "").unwrap();
            let prog = cmd.as_std().get_program();
            assert_eq!(prog, "bash");

            let args: Vec<_> = cmd.as_std().get_args().collect();
            assert_eq!(args[0], "-c");

            let script = args[1].to_string_lossy();
            assert!(script.contains("export CANGJIE_HOME="));
            assert!(script.contains("source"));
            assert!(script.contains("envsetup.sh"));
            assert!(script.contains("exec"));
            assert!(script.contains("LSPServer"));
            assert!(!script.contains("export PATH="));
        }

        #[test]
        fn test_build_unix_command_with_require_path() {
            let settings = test_settings("/opt/cangjie-sdk");
            let cmd = build_unix_command(&settings, "/extra/lib/path").unwrap();
            let args: Vec<_> = cmd.as_std().get_args().collect();
            let script = args[1].to_string_lossy();
            assert!(script.contains("export PATH="));
            assert!(script.contains("/extra/lib/path"));
            assert!(script.contains("\"$PATH\""));
        }

        #[test]
        fn test_build_unix_command_with_spaces_in_path() {
            let settings = test_settings("/opt/my sdk/cangjie");
            let cmd = build_unix_command(&settings, "/my lib/path").unwrap();
            let args: Vec<_> = cmd.as_std().get_args().collect();
            let script = args[1].to_string_lossy();
            // shlex should quote paths with spaces
            assert!(script.contains("CANGJIE_HOME="));
            assert!(script.contains("my sdk"));
            assert!(script.contains("my lib"));
        }

        #[test]
        fn test_build_unix_command_includes_lsp_args() {
            let settings = LSPSettings {
                sdk_path: PathBuf::from("/opt/sdk"),
                workspace_path: PathBuf::from("/ws"),
                log_enabled: false,
                log_path: None,
                init_timeout_ms: 30000,
                disable_auto_import: true,
            };
            let cmd = build_unix_command(&settings, "").unwrap();
            let args: Vec<_> = cmd.as_std().get_args().collect();
            let script = args[1].to_string_lossy();
            assert!(script.contains("src"));
            assert!(script.contains("--disableAutoImport"));
            assert!(script.contains("--enable-log=false"));
        }

        #[test]
        fn test_build_shell_command_dispatches_to_unix() {
            let settings = test_settings("/opt/sdk");
            let cmd = build_shell_command(&settings, "").unwrap();
            assert_eq!(cmd.as_std().get_program(), "bash");
        }
    }

    #[cfg(windows)]
    mod windows_tests {
        use super::*;

        #[test]
        fn test_build_windows_direct_command_basic() {
            let settings = test_settings("C:\\cangjie-sdk");
            let env = HashMap::from([
                ("Path".to_string(), "C:\\Windows".to_string()),
                ("CANGJIE_HOME".to_string(), "C:\\cangjie-sdk".to_string()),
            ]);
            let cmd = build_windows_direct_command(&settings, "", env);
            let prog = cmd.as_std().get_program().to_string_lossy().to_string();
            assert!(
                prog.contains("LSPServer"),
                "should launch LSPServer directly, got: {}",
                prog
            );
            assert!(
                !prog.to_lowercase().contains("powershell"),
                "should not use powershell as program, got: {}",
                prog
            );
        }

        #[test]
        fn test_build_windows_direct_command_with_require_path() {
            let settings = test_settings("C:\\cangjie-sdk");
            let env = HashMap::from([("Path".to_string(), "C:\\Windows".to_string())]);
            let cmd = build_windows_direct_command(&settings, "C:\\extra\\lib", env);
            let envs: HashMap<_, _> = cmd
                .as_std()
                .get_envs()
                .filter_map(|(k, v)| {
                    Some((
                        k.to_string_lossy().to_string(),
                        v?.to_string_lossy().to_string(),
                    ))
                })
                .collect();
            let path = envs.get("Path").expect("Path should be set");
            assert!(
                path.starts_with("C:\\extra\\lib;"),
                "Path should be prepended, got: {}",
                path
            );
        }

        #[test]
        fn test_build_windows_direct_command_includes_lsp_args() {
            let settings = LSPSettings {
                sdk_path: PathBuf::from("C:\\sdk"),
                workspace_path: PathBuf::from("C:\\ws"),
                log_enabled: false,
                log_path: None,
                init_timeout_ms: 30000,
                disable_auto_import: true,
            };
            let env = HashMap::new();
            let cmd = build_windows_direct_command(&settings, "", env);
            let args: Vec<_> = cmd
                .as_std()
                .get_args()
                .map(|a| a.to_string_lossy().to_string())
                .collect();
            assert!(args.contains(&"src".to_string()));
            assert!(args.contains(&"--disableAutoImport".to_string()));
            assert!(args.contains(&"--enable-log=false".to_string()));
        }

        #[test]
        fn test_build_windows_direct_command_path_case_insensitive() {
            let settings = test_settings("C:\\cangjie-sdk");
            // Use "PATH" (all caps) instead of "Path"
            let env = HashMap::from([("PATH".to_string(), "C:\\Windows".to_string())]);
            let cmd = build_windows_direct_command(&settings, "C:\\extra", env);
            let envs: HashMap<_, _> = cmd
                .as_std()
                .get_envs()
                .filter_map(|(k, v)| {
                    Some((
                        k.to_string_lossy().to_string(),
                        v?.to_string_lossy().to_string(),
                    ))
                })
                .collect();
            let path = envs.get("PATH").expect("PATH should be set");
            assert!(
                path.starts_with("C:\\extra;"),
                "PATH should be prepended, got: {}",
                path
            );
        }

        #[test]
        fn test_build_windows_direct_command_env_clear() {
            let settings = test_settings("C:\\cangjie-sdk");
            let env = HashMap::from([("MY_VAR".to_string(), "my_value".to_string())]);
            let cmd = build_windows_direct_command(&settings, "", env);
            let envs: HashMap<_, _> = cmd
                .as_std()
                .get_envs()
                .filter_map(|(k, v)| {
                    Some((
                        k.to_string_lossy().to_string(),
                        v?.to_string_lossy().to_string(),
                    ))
                })
                .collect();
            assert_eq!(
                envs.get("MY_VAR").map(|s| s.as_str()),
                Some("my_value"),
                "env vars from captured environment should be set"
            );
        }
    }
}
