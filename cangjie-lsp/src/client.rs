use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::RwLock as StdRwLock;
use std::time::Duration;

use anyhow::{Context, Result};
use jsonrpsee::core::client::ClientT;
use serde_json::Value;
use tokio::sync::{mpsc, Mutex, Notify};
use tracing::{debug, info, warn};

use crate::config::{LSPInitOptions, LSPSettings};
use crate::transport::{
    process_monitor, stderr_task, stdin_task, stdout_reader_task, LspParams, LspReceiver, LspSender,
};
use crate::types::{
    CallHierarchyIncomingCallsParams, CallHierarchyOutgoingCallsParams, CallHierarchyPrepareParams,
    ClientInfo, DidChangeTextDocumentParams, DidOpenTextDocumentParams, DocumentSymbolParams,
    GotoDefinitionParams, HoverParams, InitializeParams, InitializedParams, Position,
    ReferenceContext, ReferenceParams, TextDocumentContentChangeEvent, TextDocumentIdentifier,
    TextDocumentItem, TextDocumentPositionParams, TraceValue, TypeHierarchyPrepareParams,
    TypeHierarchySubtypesParams, TypeHierarchySupertypesParams, Uri,
    VersionedTextDocumentIdentifier, WorkDoneProgressParams, WorkspaceFolder,
    WorkspaceSymbolParams,
};
use crate::utils::{path_to_uri, uri_to_path};

mod capabilities;
mod command;

pub use capabilities::SupportedOperation;

use capabilities::{build_client_capabilities, supports_capability};
use command::build_shell_command;

const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
const DIAGNOSTIC_WAIT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone)]
enum ClientRuntimeState {
    Starting,
    Ready { raw_capabilities: Box<Value> },
    Shutdown,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DiagnosticsStatus {
    #[default]
    Ready,
    Timeout,
}

#[derive(Debug, Clone, Default)]
pub struct DiagnosticsResponse {
    pub diagnostics: Vec<Value>,
    pub status: DiagnosticsStatus,
}

// -- Helpers -----------------------------------------------------------------

fn parse_uri(s: &str) -> Result<Uri> {
    s.parse()
        .map_err(|e| anyhow::anyhow!("Invalid URI '{s}': {e}"))
}

fn make_td_position(uri: Uri, line: u32, character: u32) -> TextDocumentPositionParams {
    TextDocumentPositionParams {
        text_document: TextDocumentIdentifier { uri },
        position: Position { line, character },
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileSyncAction {
    DidOpen { version: i32 },
    DidChange { version: i32 },
    Noop,
}

fn next_file_sync_action(previous_version: Option<i32>, content_changed: bool) -> FileSyncAction {
    match (previous_version, content_changed) {
        (None, _) => FileSyncAction::DidOpen { version: 1 },
        (Some(version), true) => FileSyncAction::DidChange {
            version: version + 1,
        },
        (Some(_), false) => FileSyncAction::Noop,
    }
}

// -- Client ------------------------------------------------------------------

pub struct CangjieClient {
    client: jsonrpsee::core::client::Client,
    open_files: Mutex<HashMap<String, i32>>,
    file_hashes: Mutex<HashMap<String, u64>>,
    diagnostics: Arc<Mutex<HashMap<String, Vec<Value>>>>,
    diagnostic_versions: Arc<Mutex<HashMap<String, u64>>>,
    diagnostics_notify: Arc<Notify>,
    runtime: StdRwLock<ClientRuntimeState>,
    running: Arc<AtomicBool>,
}

impl CangjieClient {
    pub async fn start(
        settings: &LSPSettings,
        init_options: &LSPInitOptions,
        require_path: &str,
    ) -> Result<Self> {
        let mut cmd = build_shell_command(settings, require_path)?;

        info!(
            "Starting LSP server via shell wrapper (sdk={})",
            settings.sdk_path.display()
        );

        let mut child = cmd
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .current_dir(&settings.workspace_path)
            .spawn()
            .context("Failed to start LSP server process")?;

        let stdin = child.stdin.take().context("No stdin")?;
        let stdout = child.stdout.take().context("No stdout")?;
        let stderr = child.stderr.take().context("No stderr")?;

        let (outbound_tx, outbound_rx) = mpsc::unbounded_channel::<String>();
        let (incoming_tx, incoming_rx) = mpsc::unbounded_channel::<String>();
        let diagnostics = Arc::new(Mutex::new(HashMap::<String, Vec<Value>>::new()));
        let diagnostic_versions = Arc::new(Mutex::new(HashMap::<String, u64>::new()));
        let diagnostics_notify = Arc::new(Notify::new());
        let running = Arc::new(AtomicBool::new(true));

        tokio::spawn(stdout_reader_task(stdout, incoming_tx, running.clone()));

        let sender = LspSender {
            outbound_tx: outbound_tx.clone(),
        };
        let receiver = LspReceiver {
            incoming_rx,
            outbound_tx: outbound_tx.clone(),
            diagnostics: diagnostics.clone(),
            diagnostic_versions: diagnostic_versions.clone(),
            diagnostics_notify: diagnostics_notify.clone(),
        };

        let rpc_client = jsonrpsee::core::client::ClientBuilder::default()
            .request_timeout(REQUEST_TIMEOUT)
            .build_with_tokio(sender, receiver);

        // Stdin writer: frames outbound messages with Content-Length headers.
        tokio::spawn(stdin_task(stdin, outbound_rx, running.clone()));
        tokio::spawn(stderr_task(stderr));
        tokio::spawn(process_monitor(child, running.clone()));

        let client = Self {
            client: rpc_client,
            open_files: Mutex::new(HashMap::new()),
            file_hashes: Mutex::new(HashMap::new()),
            diagnostics,
            diagnostic_versions,
            diagnostics_notify,
            runtime: StdRwLock::new(ClientRuntimeState::Starting),
            running,
        };

        client.lsp_initialize(settings, init_options).await?;

        Ok(client)
    }

    async fn lsp_initialize(
        &self,
        settings: &LSPSettings,
        init_options: &LSPInitOptions,
    ) -> Result<()> {
        let root_uri = parse_uri(&path_to_uri(&settings.workspace_path))?;

        let workspace_name = settings
            .workspace_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "workspace".to_string());

        let capabilities = build_client_capabilities();

        #[allow(deprecated)] // root_uri and root_path are needed by many LSP servers
        let init_params = InitializeParams {
            process_id: Some(std::process::id()),
            client_info: Some(ClientInfo {
                name: "cangjie-mcp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
            root_uri: Some(root_uri.clone()),
            root_path: Some(settings.workspace_path.to_string_lossy().to_string()),
            workspace_folders: Some(vec![WorkspaceFolder {
                uri: root_uri,
                name: workspace_name,
            }]),
            initialization_options: Some(serde_json::to_value(init_options)?),
            capabilities,
            trace: Some(TraceValue::Off),
            ..Default::default()
        };

        debug!("[LSP] initialize");
        let init_result_value: Value = self
            .client
            .request("initialize", LspParams::new(&init_params)?)
            .await
            .map_err(|e| anyhow::anyhow!("LSP initialization failed: {e}"))?;

        debug!("[LSP] initialized");
        self.client
            .notification("initialized", LspParams::new(&InitializedParams {})?)
            .await
            .map_err(|e| anyhow::anyhow!("LSP initialized notification failed: {e}"))?;

        let raw_capabilities = init_result_value
            .get("capabilities")
            .cloned()
            .unwrap_or(Value::Null);
        let mut runtime = self.runtime.write().unwrap_or_else(|e| e.into_inner());
        *runtime = ClientRuntimeState::Ready {
            raw_capabilities: Box::new(raw_capabilities),
        };
        info!("LSP client initialized successfully");
        Ok(())
    }

    pub fn is_initialized(&self) -> bool {
        matches!(
            *self.runtime.read().unwrap_or_else(|e| e.into_inner()),
            ClientRuntimeState::Ready { .. }
        )
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst) && self.client.is_connected()
    }

    pub fn is_alive(&self) -> bool {
        self.is_initialized() && self.is_running()
    }

    pub fn supports(&self, operation: SupportedOperation) -> bool {
        let runtime = self.runtime.read().unwrap_or_else(|e| e.into_inner());
        let ClientRuntimeState::Ready {
            raw_capabilities, ..
        } = &*runtime
        else {
            return false;
        };
        supports_capability(raw_capabilities.as_ref(), operation)
    }

    // -- Request / notification helpers --------------------------------------

    /// Send a `textDocument/*` request and concurrently fire a lightweight
    /// `textDocument/documentLink` request for the **same URI** to unblock
    /// the Cangjie LSP server's response pipeline.
    ///
    /// The Cangjie LSP server only triggers its `ReadyForDiagnostics` step
    /// (which flushes queued responses for a given file) when the **next**
    /// handled request for that file arrives.  Cursor/VSCode naturally sends
    /// `documentLink` right after `documentSymbol`, providing that kick.
    /// We replicate the same behaviour here.
    async fn document_request<P: serde::Serialize>(
        &self,
        method: &str,
        params: &P,
        uri: &Uri,
    ) -> Result<Value> {
        let rpc_params = LspParams::new(params)?;
        let client = &self.client;

        let request_fut = client.request::<Value, _>(method, rpc_params);

        let kick_fut = async {
            // No delay — the kick must land in the same stdin read buffer as
            // the main request so the server processes both in the same
            // ArkASTWorker::Update cycle.  tokio::join! starts both futures
            // concurrently; the underlying stdin writer serialises the writes.
            let kick_params = LspParams::new(&serde_json::json!({
                "textDocument": { "uri": uri.as_str() }
            }))
            .unwrap_or_else(|_| LspParams::empty());
            let _: std::result::Result<Value, _> = client
                .request("textDocument/documentLink", kick_params)
                .await;
        };

        let (result, _) = tokio::join!(request_fut, kick_fut);
        result.map_err(|e| anyhow::anyhow!("LSP request '{method}' failed: {e}"))
    }

    async fn notify<P: serde::Serialize>(&self, method: &str, params: &P) -> Result<()> {
        self.client
            .notification(method, LspParams::new(params)?)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(())
    }

    // -- File management -----------------------------------------------------

    fn hash_content(content: &str) -> u64 {
        let mut hasher = DefaultHasher::new();
        content.hash(&mut hasher);
        hasher.finish()
    }

    async fn ensure_open(&self, file_path: &str) -> Result<()> {
        let uri_str = path_to_uri(Path::new(file_path));
        let content = tokio::fs::read_to_string(file_path)
            .await
            .context("Failed to read file")?;
        let content_hash = Self::hash_content(&content);
        let previous_version = self.open_files.lock().await.get(&uri_str).copied();
        let previous_hash = self.file_hashes.lock().await.get(&uri_str).copied();
        let content_changed = previous_hash != Some(content_hash);
        let sync_action = next_file_sync_action(previous_version, content_changed);

        match sync_action {
            FileSyncAction::DidOpen { version } => {
                let params = DidOpenTextDocumentParams {
                    text_document: TextDocumentItem {
                        uri: parse_uri(&uri_str)?,
                        language_id: "Cangjie".to_string(),
                        version,
                        text: content,
                    },
                };

                debug!("[LSP] textDocument/didOpen: {}", uri_str);
                self.notify("textDocument/didOpen", &params).await?;
                self.open_files
                    .lock()
                    .await
                    .insert(uri_str.clone(), version);
                self.file_hashes.lock().await.insert(uri_str, content_hash);
            }
            FileSyncAction::DidChange { version } => {
                let params = DidChangeTextDocumentParams {
                    text_document: VersionedTextDocumentIdentifier {
                        uri: parse_uri(&uri_str)?,
                        version,
                    },
                    content_changes: vec![TextDocumentContentChangeEvent {
                        range: None,
                        range_length: None,
                        text: content,
                    }],
                };

                debug!("[LSP] textDocument/didChange: {}", uri_str);
                self.notify("textDocument/didChange", &params).await?;
                self.open_files
                    .lock()
                    .await
                    .insert(uri_str.clone(), version);
                self.file_hashes.lock().await.insert(uri_str, content_hash);
            }
            FileSyncAction::Noop => {}
        }
        Ok(())
    }

    // -- LSP operations ------------------------------------------------------

    pub async fn definition(&self, file_path: &str, line: u32, character: u32) -> Result<Value> {
        self.ensure_open(file_path).await?;
        let uri = parse_uri(&path_to_uri(Path::new(file_path)))?;
        self.document_request(
            "textDocument/definition",
            &GotoDefinitionParams {
                text_document_position_params: make_td_position(uri.clone(), line, character),
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: Default::default(),
            },
            &uri,
        )
        .await
    }

    pub async fn references(&self, file_path: &str, line: u32, character: u32) -> Result<Value> {
        self.ensure_open(file_path).await?;
        let uri = parse_uri(&path_to_uri(Path::new(file_path)))?;
        self.document_request(
            "textDocument/references",
            &ReferenceParams {
                text_document_position: make_td_position(uri.clone(), line, character),
                context: ReferenceContext {
                    include_declaration: true,
                },
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: Default::default(),
            },
            &uri,
        )
        .await
    }

    pub async fn hover(&self, file_path: &str, line: u32, character: u32) -> Result<Value> {
        self.ensure_open(file_path).await?;
        let uri = parse_uri(&path_to_uri(Path::new(file_path)))?;
        self.document_request(
            "textDocument/hover",
            &HoverParams {
                text_document_position_params: make_td_position(uri.clone(), line, character),
                work_done_progress_params: WorkDoneProgressParams::default(),
            },
            &uri,
        )
        .await
    }

    pub async fn document_symbol(&self, file_path: &str) -> Result<Value> {
        self.ensure_open(file_path).await?;
        let uri = parse_uri(&path_to_uri(Path::new(file_path)))?;
        self.document_request(
            "textDocument/documentSymbol",
            &DocumentSymbolParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: Default::default(),
            },
            &uri,
        )
        .await
    }

    pub async fn workspace_symbol(&self, query: &str) -> Result<Value> {
        let params = WorkspaceSymbolParams {
            query: query.to_string(),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: Default::default(),
        };
        let rpc_params = LspParams::new(&params)?;
        self.client
            .request::<Value, _>("workspace/symbol", rpc_params)
            .await
            .map_err(|e| anyhow::anyhow!("LSP workspace/symbol failed: {e}"))
    }

    pub async fn incoming_calls(
        &self,
        file_path: &str,
        line: u32,
        character: u32,
    ) -> Result<Value> {
        self.ensure_open(file_path).await?;
        let uri = parse_uri(&path_to_uri(Path::new(file_path)))?;
        let prepare_params = CallHierarchyPrepareParams {
            text_document_position_params: make_td_position(uri.clone(), line, character),
            work_done_progress_params: WorkDoneProgressParams::default(),
        };
        let items: Value = self
            .document_request("textDocument/prepareCallHierarchy", &prepare_params, &uri)
            .await?;
        let items_arr = items.as_array().cloned().unwrap_or_default();
        if items_arr.is_empty() {
            return Ok(Value::Array(vec![]));
        }
        let item = serde_json::from_value(items_arr[0].clone())?;
        let params = CallHierarchyIncomingCallsParams {
            item,
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: Default::default(),
        };
        let rpc_params = LspParams::new(&params)?;
        self.client
            .request::<Value, _>("callHierarchy/incomingCalls", rpc_params)
            .await
            .map_err(|e| anyhow::anyhow!("LSP callHierarchy/incomingCalls failed: {e}"))
    }

    pub async fn outgoing_calls(
        &self,
        file_path: &str,
        line: u32,
        character: u32,
    ) -> Result<Value> {
        self.ensure_open(file_path).await?;
        let uri = parse_uri(&path_to_uri(Path::new(file_path)))?;
        let prepare_params = CallHierarchyPrepareParams {
            text_document_position_params: make_td_position(uri.clone(), line, character),
            work_done_progress_params: WorkDoneProgressParams::default(),
        };
        let items: Value = self
            .document_request("textDocument/prepareCallHierarchy", &prepare_params, &uri)
            .await?;
        let items_arr = items.as_array().cloned().unwrap_or_default();
        if items_arr.is_empty() {
            return Ok(Value::Array(vec![]));
        }
        let item = serde_json::from_value(items_arr[0].clone())?;
        let params = CallHierarchyOutgoingCallsParams {
            item,
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: Default::default(),
        };
        let rpc_params = LspParams::new(&params)?;
        self.client
            .request::<Value, _>("callHierarchy/outgoingCalls", rpc_params)
            .await
            .map_err(|e| anyhow::anyhow!("LSP callHierarchy/outgoingCalls failed: {e}"))
    }

    pub async fn type_supertypes(
        &self,
        file_path: &str,
        line: u32,
        character: u32,
    ) -> Result<Value> {
        self.ensure_open(file_path).await?;
        let uri = parse_uri(&path_to_uri(Path::new(file_path)))?;
        let prepare_params = TypeHierarchyPrepareParams {
            text_document_position_params: make_td_position(uri.clone(), line, character),
            work_done_progress_params: WorkDoneProgressParams::default(),
        };
        let items: Value = self
            .document_request("textDocument/prepareTypeHierarchy", &prepare_params, &uri)
            .await?;
        let items_arr = items.as_array().cloned().unwrap_or_default();
        if items_arr.is_empty() {
            return Ok(Value::Array(vec![]));
        }
        let item = serde_json::from_value(items_arr[0].clone())?;
        let params = TypeHierarchySupertypesParams {
            item,
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: Default::default(),
        };
        let rpc_params = LspParams::new(&params)?;
        self.client
            .request::<Value, _>("typeHierarchy/supertypes", rpc_params)
            .await
            .map_err(|e| anyhow::anyhow!("LSP typeHierarchy/supertypes failed: {e}"))
    }

    pub async fn type_subtypes(&self, file_path: &str, line: u32, character: u32) -> Result<Value> {
        self.ensure_open(file_path).await?;
        let uri = parse_uri(&path_to_uri(Path::new(file_path)))?;
        let prepare_params = TypeHierarchyPrepareParams {
            text_document_position_params: make_td_position(uri.clone(), line, character),
            work_done_progress_params: WorkDoneProgressParams::default(),
        };
        let items: Value = self
            .document_request("textDocument/prepareTypeHierarchy", &prepare_params, &uri)
            .await?;
        let items_arr = items.as_array().cloned().unwrap_or_default();
        if items_arr.is_empty() {
            return Ok(Value::Array(vec![]));
        }
        let item = serde_json::from_value(items_arr[0].clone())?;
        let params = TypeHierarchySubtypesParams {
            item,
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: Default::default(),
        };
        let rpc_params = LspParams::new(&params)?;
        self.client
            .request::<Value, _>("typeHierarchy/subtypes", rpc_params)
            .await
            .map_err(|e| anyhow::anyhow!("LSP typeHierarchy/subtypes failed: {e}"))
    }

    fn diagnostics_key(file_path: &str) -> String {
        uri_to_path(path_to_uri(Path::new(file_path)).as_str())
            .to_string_lossy()
            .to_string()
    }

    async fn diagnostics_version(&self, key: &str) -> u64 {
        self.diagnostic_versions
            .lock()
            .await
            .get(key)
            .copied()
            .unwrap_or(0)
    }

    async fn wait_for_diagnostics(
        &self,
        key: &str,
        previous_version: u64,
        timeout: Duration,
    ) -> DiagnosticsStatus {
        let deadline = tokio::time::Instant::now() + timeout;

        loop {
            if self.diagnostics_version(key).await > previous_version {
                return DiagnosticsStatus::Ready;
            }

            let notified = self.diagnostics_notify.notified();
            tokio::select! {
                _ = notified => {}
                _ = tokio::time::sleep_until(deadline) => return DiagnosticsStatus::Timeout,
            }
        }
    }

    pub async fn get_diagnostics(&self, file_path: &str) -> Result<DiagnosticsResponse> {
        self.ensure_open(file_path).await?;
        let lookup_key = Self::diagnostics_key(file_path);
        let previous_version = self.diagnostics_version(&lookup_key).await;

        // Send a documentSymbol request to kick the LSP server's ReadyForDiagnostics pipeline
        let uri = parse_uri(&path_to_uri(Path::new(file_path)))?;
        let _ = self
            .document_request(
                "textDocument/documentSymbol",
                &DocumentSymbolParams {
                    text_document: TextDocumentIdentifier { uri: uri.clone() },
                    work_done_progress_params: WorkDoneProgressParams::default(),
                    partial_result_params: Default::default(),
                },
                &uri,
            )
            .await;
        let status = self
            .wait_for_diagnostics(&lookup_key, previous_version, DIAGNOSTIC_WAIT_TIMEOUT)
            .await;
        let diagnostics = self
            .diagnostics
            .lock()
            .await
            .get(&lookup_key)
            .cloned()
            .unwrap_or_default();

        Ok(DiagnosticsResponse {
            diagnostics,
            status,
        })
    }

    pub async fn shutdown(&self) -> Result<()> {
        debug!("[LSP] shutdown");
        if let Err(e) = self
            .client
            .request::<Value, _>("shutdown", LspParams::empty())
            .await
        {
            warn!("LSP shutdown request failed: {}", e);
        }

        self.running.store(false, Ordering::SeqCst);

        debug!("[LSP] exit");
        if let Err(e) = self.client.notification("exit", LspParams::empty()).await {
            warn!("LSP exit notification failed: {}", e);
        }

        let mut runtime = self.runtime.write().unwrap_or_else(|e| e.into_inner());
        *runtime = ClientRuntimeState::Shutdown;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_next_file_sync_action_for_open_and_change() {
        assert_eq!(
            next_file_sync_action(None, false),
            FileSyncAction::DidOpen { version: 1 }
        );
        assert_eq!(
            next_file_sync_action(Some(1), true),
            FileSyncAction::DidChange { version: 2 }
        );
        assert_eq!(next_file_sync_action(Some(2), false), FileSyncAction::Noop);
    }
}
