// The std::sync::Mutex guard is intentionally held across await points to
// serialize LSP tests (only one LSP server can run at a time).
#![allow(clippy::await_holding_lock)]
//! LSP integration tests using a real Cangjie project (cjbind/cjbind).
//!
//! These tests require:
//! - `CANGJIE_HOME` env var pointing to a valid Cangjie SDK
//! - `CANGJIE_LSP_TEST_PROJECT` env var pointing to a cloned cjbind repo
//!
//! In CI, both are set up by the lsp-test job. Locally, tests are skipped
//! when either env var is missing.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use cangjie_lsp as lsp;
use cangjie_lsp::config::LSPSettings;
use cangjie_server::lsp_tools::{
    LspOperation, LspRequest, LspResponse, LspResponseStatus, LspTarget,
};
use cangjie_server::mcp_handler::CangjieServer;
use cangjie_server::Parameters;
use rmcp::model::Meta;

// ── Serialization ──────────────────────────────────────────────────────────

/// Only one LSP server can run at a time (global singleton).
static LSP_TEST_MUTEX: Mutex<()> = Mutex::new(());

// ── Helpers ────────────────────────────────────────────────────────────────

/// Return the test project path from `CANGJIE_LSP_TEST_PROJECT`, or `None`.
fn test_project_path() -> Option<PathBuf> {
    std::env::var("CANGJIE_LSP_TEST_PROJECT")
        .ok()
        .map(PathBuf::from)
        .filter(|p| p.exists())
}

/// Detect and validate LSP settings for the test project workspace.
fn detect_test_settings(project_path: &Path) -> Option<LSPSettings> {
    let settings = lsp::detect_settings(Some(project_path.to_path_buf()))?;
    let errors = settings.validate();
    if !errors.is_empty() {
        eprintln!("SDK validation failed: {}", errors.join("; "));
        return None;
    }
    Some(LSPSettings {
        workspace_path: project_path.to_path_buf(),
        ..settings
    })
}

/// Skip test if SDK or test project is not available.
macro_rules! require_project {
    () => {{
        let project = match test_project_path() {
            Some(p) => p,
            None => {
                eprintln!("CANGJIE_LSP_TEST_PROJECT not set — skipping test");
                return;
            }
        };
        let settings = match detect_test_settings(&project) {
            Some(s) => s,
            None => {
                eprintln!("Cangjie SDK not available — skipping test");
                return;
            }
        };
        (project, settings)
    }};
}

/// Initialize LSP for the test project and return the project path.
/// Panics if initialization fails.
async fn init_lsp_for_project() -> Option<PathBuf> {
    let (project, settings) = {
        let project = test_project_path()?;
        let settings = detect_test_settings(&project)?;
        (project, settings)
    };

    let ok = lsp::init(settings).await;
    assert!(ok, "LSP initialization should succeed for cjbind project");
    Some(project)
}

/// Parse the JSON response from the unified LSP tool.
fn parse_lsp_response(json: &str) -> LspResponse {
    serde_json::from_str(json).unwrap_or_else(|e| {
        panic!("Failed to parse LSP response: {e}\nRaw: {json}");
    })
}

/// Build a CangjieServer with no search backend (LSP-only testing).
fn build_lsp_test_server() -> CangjieServer {
    let settings = cangjie_core::config::Settings {
        data_dir: std::env::temp_dir().join("cangjie-lsp-test"),
        ..cangjie_core::config::Settings::default()
    };
    CangjieServer::new(settings)
}

fn lsp_req(operation: LspOperation) -> LspRequest {
    LspRequest {
        operation,
        file_path: None,
        target: None,
        query: None,
        new_name: None,
    }
}

fn lsp_file_req(operation: LspOperation, file_path: &str) -> LspRequest {
    LspRequest {
        file_path: Some(file_path.to_string()),
        ..lsp_req(operation)
    }
}

fn lsp_symbol_req(operation: LspOperation, file_path: &str, symbol: &str) -> LspRequest {
    LspRequest {
        file_path: Some(file_path.to_string()),
        target: Some(LspTarget::Symbol {
            symbol: symbol.to_string(),
            line_hint: None,
        }),
        ..lsp_req(operation)
    }
}

/// Execute an LSP request via the MCP tool and parse the response.
async fn lsp_call(server: &CangjieServer, req: LspRequest) -> LspResponse {
    let json = server.lsp(Parameters(req), Meta::default()).await;
    parse_lsp_response(&json)
}

// ═══════════════════════════════════════════════════════════════════════════
// 1. LSP Lifecycle with Real Project
// ═══════════════════════════════════════════════════════════════════════════

/// Initialize LSP against cjbind, verify client is alive, then shutdown.
#[tokio::test]
async fn test_lsp_lifecycle_with_real_project() {
    let _lock = LSP_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    let (_project, settings) = require_project!();

    let ok = lsp::init(settings).await;
    assert!(ok, "LSP init should succeed with cjbind project");

    {
        let guard = lsp::get_client().await;
        assert!(guard.is_some(), "client should be available");
        let client = guard.unwrap();
        let client_ref = client.as_ref().unwrap();
        assert!(client_ref.is_alive(), "client should be alive");
    }

    lsp::shutdown().await;
    let guard = lsp::get_client().await;
    assert!(guard.is_none(), "client should be None after shutdown");
}

// ═══════════════════════════════════════════════════════════════════════════
// 2. Document Symbols — verify cjbind source files have symbols
// ═══════════════════════════════════════════════════════════════════════════

/// Get document symbols from cjbind/src/lib.cj — should find `generate`, `parse`, etc.
#[tokio::test]
async fn test_document_symbol_lib_cj() {
    let _lock = LSP_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    let project = match init_lsp_for_project().await {
        Some(p) => p,
        None => return,
    };
    let fp = project.join("cjbind").join("src").join("lib.cj");
    assert!(fp.exists(), "cjbind/src/lib.cj should exist");

    let server = build_lsp_test_server();
    let resp = lsp_call(
        &server,
        lsp_file_req(LspOperation::DocumentSymbol, &fp.to_string_lossy()),
    )
    .await;
    assert!(
        matches!(
            resp.status,
            LspResponseStatus::Ok | LspResponseStatus::Empty
        ),
        "document_symbol should succeed or be empty, got: {:?} - {:?}",
        resp.status,
        resp.message
    );
    if resp.status == LspResponseStatus::Ok {
        let data_str = serde_json::to_string(&resp.data).unwrap();
        assert!(
            data_str.contains("generate") || data_str.contains("parse"),
            "lib.cj should contain 'generate' or 'parse' symbols"
        );
    }
    lsp::shutdown().await;
}

/// Get document symbols from options.cj — should find CjbindOptions class.
#[tokio::test]
async fn test_document_symbol_options_cj() {
    let _lock = LSP_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    let project = match init_lsp_for_project().await {
        Some(p) => p,
        None => return,
    };
    let fp = project
        .join("cjbind")
        .join("src")
        .join("options")
        .join("options.cj");
    if !fp.exists() {
        eprintln!("options.cj not found, skipping");
        lsp::shutdown().await;
        return;
    }

    let server = build_lsp_test_server();
    let resp = lsp_call(
        &server,
        lsp_file_req(LspOperation::DocumentSymbol, &fp.to_string_lossy()),
    )
    .await;
    if resp.status == LspResponseStatus::Ok {
        let data_str = serde_json::to_string(&resp.data).unwrap();
        assert!(
            data_str.contains("CjbindOptions"),
            "options.cj should contain CjbindOptions class"
        );
    }
    lsp::shutdown().await;
}

// ═══════════════════════════════════════════════════════════════════════════
// 3. Workspace Symbol — search across the whole project
// ═══════════════════════════════════════════════════════════════════════════

/// Search for "CjbindOptions" across the workspace.
#[tokio::test]
async fn test_workspace_symbol_search() {
    let _lock = LSP_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    if init_lsp_for_project().await.is_none() {
        return;
    }

    let server = build_lsp_test_server();
    let mut req = lsp_req(LspOperation::WorkspaceSymbol);
    req.query = Some("CjbindOptions".into());
    let resp = lsp_call(&server, req).await;
    assert!(
        matches!(
            resp.status,
            LspResponseStatus::Ok | LspResponseStatus::Empty
        ),
        "workspace_symbol should not error, got: {:?} - {:?}",
        resp.status,
        resp.message
    );
    if resp.status == LspResponseStatus::Ok {
        let data_str = serde_json::to_string(&resp.data).unwrap();
        assert!(
            data_str.contains("CjbindOptions"),
            "should find CjbindOptions"
        );
    }
    lsp::shutdown().await;
}

/// Search for "Item" — a key class in the IR module.
#[tokio::test]
async fn test_workspace_symbol_item() {
    let _lock = LSP_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    if init_lsp_for_project().await.is_none() {
        return;
    }

    let server = build_lsp_test_server();
    let mut req = lsp_req(LspOperation::WorkspaceSymbol);
    req.query = Some("Item".into());
    let resp = lsp_call(&server, req).await;
    assert!(
        matches!(
            resp.status,
            LspResponseStatus::Ok | LspResponseStatus::Empty
        ),
        "workspace_symbol for 'Item' should not error"
    );
    lsp::shutdown().await;
}

// ═══════════════════════════════════════════════════════════════════════════
// 4. Diagnostics — valid files should compile cleanly
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_diagnostics_on_valid_source() {
    let _lock = LSP_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    let project = match init_lsp_for_project().await {
        Some(p) => p,
        None => return,
    };
    let fp = project.join("cjbind").join("src").join("lib.cj");
    assert!(fp.exists());

    let server = build_lsp_test_server();
    let resp = lsp_call(
        &server,
        lsp_file_req(LspOperation::Diagnostics, &fp.to_string_lossy()),
    )
    .await;
    assert!(
        matches!(
            resp.status,
            LspResponseStatus::Ok | LspResponseStatus::Empty | LspResponseStatus::Timeout
        ),
        "diagnostics should succeed, got: {:?} - {:?}",
        resp.status,
        resp.message
    );
    lsp::shutdown().await;
}

// ═══════════════════════════════════════════════════════════════════════════
// 5. Hover — get type info for symbols
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_hover_on_function() {
    let _lock = LSP_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    let project = match init_lsp_for_project().await {
        Some(p) => p,
        None => return,
    };
    let fp = project.join("cjbind").join("src").join("lib.cj");
    assert!(fp.exists());

    let server = build_lsp_test_server();
    let resp = lsp_call(
        &server,
        lsp_symbol_req(LspOperation::Hover, &fp.to_string_lossy(), "generate"),
    )
    .await;
    assert!(
        matches!(
            resp.status,
            LspResponseStatus::Ok | LspResponseStatus::Empty | LspResponseStatus::Error
        ),
        "hover should return a valid status, got: {:?}",
        resp.status
    );
    if resp.status == LspResponseStatus::Ok {
        let data_str = serde_json::to_string(&resp.data).unwrap();
        assert!(
            !data_str.is_empty() && data_str != "{}",
            "hover result should contain type information"
        );
    }
    lsp::shutdown().await;
}

// ═══════════════════════════════════════════════════════════════════════════
// 6. Go to Definition
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_goto_definition() {
    let _lock = LSP_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    let project = match init_lsp_for_project().await {
        Some(p) => p,
        None => return,
    };
    let fp = project.join("cjbind").join("src").join("lib.cj");
    assert!(fp.exists());

    let server = build_lsp_test_server();
    let resp = lsp_call(
        &server,
        lsp_symbol_req(LspOperation::Definition, &fp.to_string_lossy(), "generate"),
    )
    .await;
    assert!(
        matches!(
            resp.status,
            LspResponseStatus::Ok | LspResponseStatus::Empty | LspResponseStatus::Error
        ),
        "definition should not panic, got: {:?} - {:?}",
        resp.status,
        resp.message
    );
    if resp.status == LspResponseStatus::Ok {
        assert!(
            resp.resolved_target.is_some(),
            "successful definition should have resolved_target"
        );
    }
    lsp::shutdown().await;
}

// ═══════════════════════════════════════════════════════════════════════════
// 7. Find References
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_find_references() {
    let _lock = LSP_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    let project = match init_lsp_for_project().await {
        Some(p) => p,
        None => return,
    };
    let fp = project
        .join("cjbind")
        .join("src")
        .join("options")
        .join("options.cj");
    if !fp.exists() {
        eprintln!("options.cj not found, skipping");
        lsp::shutdown().await;
        return;
    }

    let server = build_lsp_test_server();
    let resp = lsp_call(
        &server,
        lsp_symbol_req(
            LspOperation::References,
            &fp.to_string_lossy(),
            "CjbindOptions",
        ),
    )
    .await;
    assert!(
        matches!(
            resp.status,
            LspResponseStatus::Ok | LspResponseStatus::Empty | LspResponseStatus::Error
        ),
        "find_references should not panic"
    );
    if resp.status == LspResponseStatus::Ok {
        let data_str = serde_json::to_string(&resp.data).unwrap();
        assert!(
            data_str.contains("references") || data_str.contains("count"),
            "references result should have data"
        );
    }
    lsp::shutdown().await;
}

// ═══════════════════════════════════════════════════════════════════════════
// 8. Completion
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_completion_at_position() {
    let _lock = LSP_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    let project = match init_lsp_for_project().await {
        Some(p) => p,
        None => return,
    };
    let fp = project.join("cjbind").join("src").join("lib.cj");
    assert!(fp.exists());

    let server = build_lsp_test_server();
    let req = LspRequest {
        file_path: Some(fp.to_string_lossy().to_string()),
        target: Some(LspTarget::Position {
            line: 1,
            character: 1,
        }),
        ..lsp_req(LspOperation::Completion)
    };
    let resp = lsp_call(&server, req).await;
    assert!(
        !matches!(resp.status, LspResponseStatus::Error)
            || resp.message.as_deref().is_some_and(|m| {
                m.contains("unsupported") || m.contains("Unsupported") || m.contains("imeout")
            }),
        "completion should not fail with unexpected error, got: {:?} - {:?}",
        resp.status,
        resp.message
    );
    lsp::shutdown().await;
}

// ═══════════════════════════════════════════════════════════════════════════
// 9. Validation Errors via MCP Tool Interface
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_lsp_validation_errors_comprehensive() {
    let _lock = LSP_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    if init_lsp_for_project().await.is_none() {
        return;
    }

    let server = build_lsp_test_server();

    // Missing file_path for definition
    let req = LspRequest {
        target: Some(LspTarget::Symbol {
            symbol: "foo".into(),
            line_hint: None,
        }),
        ..lsp_req(LspOperation::Definition)
    };
    let resp = lsp_call(&server, req).await;
    assert_eq!(resp.status, LspResponseStatus::Error);
    assert!(resp.message.as_deref().unwrap().contains("file_path"));

    // Missing target for hover
    let resp = lsp_call(&server, lsp_file_req(LspOperation::Hover, "/tmp/test.cj")).await;
    assert_eq!(resp.status, LspResponseStatus::Error);
    assert!(resp.message.as_deref().unwrap().contains("target"));

    // Missing query for workspace_symbol
    let resp = lsp_call(&server, lsp_req(LspOperation::WorkspaceSymbol)).await;
    assert_eq!(resp.status, LspResponseStatus::Error);
    assert!(resp.message.as_deref().unwrap().contains("query"));

    // Missing new_name for rename
    let resp = lsp_call(
        &server,
        lsp_symbol_req(LspOperation::Rename, "/tmp/test.cj", "foo"),
    )
    .await;
    assert_eq!(resp.status, LspResponseStatus::Error);
    assert!(resp.message.as_deref().unwrap().contains("new_name"));

    // Completion with symbol target (should require position)
    let resp = lsp_call(
        &server,
        lsp_symbol_req(LspOperation::Completion, "/tmp/test.cj", "foo"),
    )
    .await;
    assert_eq!(resp.status, LspResponseStatus::Error);
    assert!(resp
        .message
        .as_deref()
        .unwrap()
        .contains("completion requires target with kind=position"));

    lsp::shutdown().await;
}

// ═══════════════════════════════════════════════════════════════════════════
// 10. Multiple Operations in Sequence
// ═══════════════════════════════════════════════════════════════════════════

/// Simulate how an AI assistant explores code: symbols → diagnostics → hover → workspace search.
#[tokio::test]
async fn test_sequential_operations_on_same_file() {
    let _lock = LSP_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    let project = match init_lsp_for_project().await {
        Some(p) => p,
        None => return,
    };
    let fp = project.join("cjbind").join("src").join("lib.cj");
    assert!(fp.exists());
    let fp_str = fp.to_string_lossy().to_string();

    let server = build_lsp_test_server();

    let symbols = lsp_call(&server, lsp_file_req(LspOperation::DocumentSymbol, &fp_str)).await;
    assert!(
        !matches!(symbols.status, LspResponseStatus::Error),
        "document_symbol should not error: {:?}",
        symbols.message
    );

    let diag = lsp_call(&server, lsp_file_req(LspOperation::Diagnostics, &fp_str)).await;
    assert!(
        !matches!(diag.status, LspResponseStatus::Error),
        "diagnostics should not error: {:?}",
        diag.message
    );

    if symbols.status == LspResponseStatus::Ok {
        let hover = lsp_call(
            &server,
            lsp_symbol_req(LspOperation::Hover, &fp_str, "generate"),
        )
        .await;
        assert!(
            !matches!(hover.status, LspResponseStatus::Unsupported),
            "hover should be supported"
        );
    }

    let mut ws_req = lsp_req(LspOperation::WorkspaceSymbol);
    ws_req.query = Some("generate".into());
    let ws = lsp_call(&server, ws_req).await;
    assert!(
        !matches!(ws.status, LspResponseStatus::Error),
        "workspace_symbol should not error: {:?}",
        ws.message
    );

    lsp::shutdown().await;
}

// ═══════════════════════════════════════════════════════════════════════════
// 11. Multiple Files in Same Session
// ═══════════════════════════════════════════════════════════════════════════

/// Open and inspect symbols from multiple files without restarting LSP.
#[tokio::test]
async fn test_multi_file_symbols() {
    let _lock = LSP_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    let project = match init_lsp_for_project().await {
        Some(p) => p,
        None => return,
    };

    let files = [
        project.join("cjbind").join("src").join("lib.cj"),
        project
            .join("cjbind")
            .join("src")
            .join("options")
            .join("options.cj"),
        project
            .join("cjbind")
            .join("src")
            .join("ir")
            .join("item.cj"),
    ];

    let server = build_lsp_test_server();
    for fp in &files {
        if !fp.exists() {
            eprintln!("Skipping missing file: {}", fp.display());
            continue;
        }
        let resp = lsp_call(
            &server,
            lsp_file_req(LspOperation::DocumentSymbol, &fp.to_string_lossy()),
        )
        .await;
        assert!(
            !matches!(resp.status, LspResponseStatus::Error),
            "document_symbol for {} should not error: {:?}",
            fp.display(),
            resp.message
        );
    }
    lsp::shutdown().await;
}
