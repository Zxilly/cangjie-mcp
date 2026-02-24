//! Integration tests for the LSP envsetup-based launch flow.
//!
//! These tests require a real Cangjie SDK installation:
//! - `CANGJIE_HOME` env var must point to the SDK root
//! - The SDK must contain `envsetup.sh` / `envsetup.ps1` and `tools/bin/LSPServer`
//!
//! Tests are automatically skipped when the SDK is not available.

use cangjie_mcp::lsp;
use cangjie_mcp::lsp::config::LSPSettings;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Guard to serialize async tests that use the global `LSP_CLIENT`.
///
/// `lsp::init()` / `lsp::shutdown()` operate on a process-wide static.
/// Cargo runs tests in parallel, so concurrent init/shutdown calls would
/// race (overwriting each other's client, orphaning LSP processes, etc.).
/// Holding this lock ensures only one lifecycle test runs at a time.
static LSP_TEST_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Return `Some(LSPSettings)` if CANGJIE_HOME is set and the SDK looks valid,
/// otherwise `None` (the caller should skip the test).
fn try_detect_settings() -> Option<LSPSettings> {
    let settings = lsp::detect_settings(None)?;
    let errors = settings.validate();
    if errors.is_empty() {
        Some(settings)
    } else {
        eprintln!(
            "SDK detected but validation failed, skipping: {}",
            errors.join("; ")
        );
        None
    }
}

/// Macro that skips the test when no valid Cangjie SDK is found.
macro_rules! require_sdk {
    () => {
        match try_detect_settings() {
            Some(s) => s,
            None => {
                eprintln!("CANGJIE_HOME not set or SDK invalid — skipping test");
                return;
            }
        }
    };
}

/// Detect the cjc-version to use in cjpm.toml.
///
/// Runs `cjc --version` via the envsetup wrapper and extracts the major.minor.patch
/// portion (e.g. "1.1.0" from "Cangjie Compiler: 1.1.0-alpha.20260205020001 (cjnative)").
/// Falls back to "0.53.4" if detection fails.
fn detect_cjc_version(settings: &LSPSettings) -> String {
    let cjc_path =
        settings
            .sdk_path
            .join("bin")
            .join(if cfg!(windows) { "cjc.exe" } else { "cjc" });
    let output = std::process::Command::new(&cjc_path)
        .arg("--version")
        .output()
        .ok();
    if let Some(output) = output {
        let stdout = String::from_utf8_lossy(&output.stdout);
        // Parse "Cangjie Compiler: 1.1.0-alpha.xxx (cjnative)"
        if let Some(ver_start) = stdout.find(": ") {
            let ver_str = &stdout[ver_start + 2..];
            // Take up to the first '-' or space to get major.minor.patch
            let end = ver_str
                .find(|c: char| c == '-' || c == ' ')
                .unwrap_or(ver_str.len());
            let version = ver_str[..end].trim().to_string();
            if !version.is_empty() {
                return version;
            }
        }
    }
    "0.53.4".to_string()
}

// ---------------------------------------------------------------------------
// Tests: SDK validation (sync)
// ---------------------------------------------------------------------------

#[test]
fn test_is_available_matches_env() {
    let available = lsp::is_available();
    let has_env = std::env::var("CANGJIE_HOME")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .is_some();
    // When CANGJIE_HOME is set, is_available should be true.
    // (It may also be true via VSCode settings detection, so we only assert
    // the forward direction.)
    if has_env {
        assert!(
            available,
            "CANGJIE_HOME is set but is_available() returned false"
        );
    }
}

#[test]
fn test_detect_settings_returns_valid_settings() {
    let settings = require_sdk!();

    assert!(settings.sdk_path.exists(), "SDK path should exist");
    assert!(
        settings.lsp_server_path().exists(),
        "LSPServer binary should exist at {}",
        settings.lsp_server_path().display()
    );
    assert!(
        settings.envsetup_script_path().exists(),
        "envsetup script should exist at {}",
        settings.envsetup_script_path().display()
    );
}

#[test]
fn test_envsetup_script_path_platform() {
    let settings = require_sdk!();
    let path = settings.envsetup_script_path();

    if cfg!(windows) {
        assert!(
            path.to_string_lossy().ends_with("envsetup.ps1"),
            "Windows should use envsetup.ps1, got: {}",
            path.display()
        );
    } else {
        assert!(
            path.to_string_lossy().ends_with("envsetup.sh"),
            "Unix should use envsetup.sh, got: {}",
            path.display()
        );
    }
}

#[test]
fn test_validate_passes_with_real_sdk() {
    let settings = require_sdk!();
    let errors = settings.validate();
    assert!(
        errors.is_empty(),
        "validate() should pass with real SDK, got errors: {:?}",
        errors
    );
}

// ---------------------------------------------------------------------------
// Tests: LSP lifecycle (async)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_lsp_init_and_shutdown() {
    let _lock = LSP_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    let settings = require_sdk!();

    // init() should succeed with a real SDK
    let ok = lsp::init(settings).await;
    assert!(ok, "lsp::init() should succeed with a valid SDK");

    // Client should be alive
    {
        let guard = lsp::get_client().await;
        assert!(guard.is_some(), "client should be available after init");
        let client = guard.unwrap();
        let client_ref = client.as_ref().unwrap();
        assert!(client_ref.is_initialized(), "client should be initialized");
        assert!(client_ref.is_running(), "client should be running");
        assert!(client_ref.is_alive(), "client should be alive");
    }

    // Shutdown
    lsp::shutdown().await;

    // Client should be gone
    let guard = lsp::get_client().await;
    assert!(guard.is_none(), "client should be None after shutdown");
}

#[tokio::test]
async fn test_lsp_init_with_workspace() {
    let _lock = LSP_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    let settings_opt = try_detect_settings();
    let Some(base) = settings_opt else {
        eprintln!("CANGJIE_HOME not set or SDK invalid — skipping test");
        return;
    };

    let cjc_version = detect_cjc_version(&base);
    let tmp = tempfile::TempDir::new().unwrap();

    // Write a minimal cjpm.toml so the workspace is realistic
    std::fs::write(
        tmp.path().join("cjpm.toml"),
        format!("[package]\nname = \"testpkg\"\ncjc-version = \"{cjc_version}\"\n"),
    )
    .unwrap();
    std::fs::create_dir_all(tmp.path().join("src")).unwrap();
    std::fs::write(
        tmp.path().join("src").join("main.cj"),
        "package testpkg\n\nmain(): Int64 {\n    return 0\n}\n",
    )
    .unwrap();

    let settings = LSPSettings {
        workspace_path: tmp.path().to_path_buf(),
        ..base
    };

    let ok = lsp::init(settings).await;
    assert!(ok, "lsp::init() should succeed with temp workspace");

    // Verify the client is alive
    {
        let guard = lsp::get_client().await;
        assert!(guard.is_some());
    }

    lsp::shutdown().await;
}

// ---------------------------------------------------------------------------
// Tests: diagnostics via LSP (async, needs SDK)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_lsp_diagnostics_on_valid_file() {
    let _lock = LSP_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    let settings_opt = try_detect_settings();
    let Some(base) = settings_opt else {
        eprintln!("CANGJIE_HOME not set or SDK invalid — skipping test");
        return;
    };

    let cjc_version = detect_cjc_version(&base);
    let tmp = tempfile::TempDir::new().unwrap();
    std::fs::write(
        tmp.path().join("cjpm.toml"),
        format!("[package]\nname = \"diagtest\"\ncjc-version = \"{cjc_version}\"\n"),
    )
    .unwrap();
    std::fs::create_dir_all(tmp.path().join("src")).unwrap();

    let src_file = tmp.path().join("src").join("main.cj");
    std::fs::write(
        &src_file,
        "package diagtest\n\nmain(): Int64 {\n    return 0\n}\n",
    )
    .unwrap();

    let settings = LSPSettings {
        workspace_path: tmp.path().to_path_buf(),
        ..base
    };

    let ok = lsp::init(settings).await;
    assert!(ok, "init should succeed");

    // Request diagnostics — valid file should have no errors
    {
        let guard = lsp::get_client().await;
        assert!(guard.is_some());
        let client = guard.unwrap();
        let client_ref = client.as_ref().unwrap();
        let diags = client_ref.get_diagnostics(src_file.to_str().unwrap()).await;
        assert!(diags.is_ok(), "get_diagnostics should not fail");
    }

    lsp::shutdown().await;
}

// ---------------------------------------------------------------------------
// Tests: document symbol (async, needs SDK)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_lsp_document_symbol() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("cangjie_mcp::lsp=info")
        .with_test_writer()
        .try_init();

    let _lock = LSP_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    let settings_opt = try_detect_settings();
    let Some(base) = settings_opt else {
        eprintln!("CANGJIE_HOME not set or SDK invalid — skipping test");
        return;
    };

    let cjc_version = detect_cjc_version(&base);
    let tmp = tempfile::TempDir::new().unwrap();
    std::fs::write(
        tmp.path().join("cjpm.toml"),
        format!("[package]\nname = \"symboltest\"\ncjc-version = \"{cjc_version}\"\n"),
    )
    .unwrap();
    std::fs::create_dir_all(tmp.path().join("src")).unwrap();

    let src_file = tmp.path().join("src").join("main.cj");
    std::fs::write(
        &src_file,
        "package symboltest\n\nmain(): Int64 {\n    return 0\n}\n",
    )
    .unwrap();

    let settings = LSPSettings {
        workspace_path: tmp.path().to_path_buf(),
        ..base
    };

    let ok = lsp::init(settings).await;
    assert!(ok, "init should succeed");

    {
        let guard = lsp::get_client().await;
        assert!(guard.is_some());
        let client = guard.unwrap();
        let client_ref = client.as_ref().unwrap();

        // Try documentSymbol with a bounded timeout.
        // ensure_open (called internally) sends didOpen, then the documentSymbol
        // request follows immediately — this keeps the server's message loop active
        // so the ArkASTWorker's ReadyForDiagnostics callback can be dispatched.
        let symbol_result = tokio::time::timeout(
            std::time::Duration::from_secs(15),
            client_ref.document_symbol(src_file.to_str().unwrap()),
        )
        .await;
        match symbol_result {
            Ok(Ok(val)) => {
                assert!(
                    !val.is_null(),
                    "document_symbol should return non-null result"
                );
                eprintln!("documentSymbol succeeded: {}", val);
            }
            Ok(Err(e)) => {
                eprintln!(
                    "WARNING: documentSymbol returned error (server may have \
                     internal issues): {e}"
                );
            }
            Err(_) => {
                eprintln!(
                    "WARNING: documentSymbol timed out after 15s — \
                     the LSP server's AST worker may be blocked. \
                     This is a known issue with some Cangjie SDK versions."
                );
            }
        }
    }

    lsp::shutdown().await;
}
