use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use serde_json::{json, Value};
use tokio::process::Command;
use tokio::sync::Mutex;
use tracing::{debug, info};

use tokio_lsp::types::{ClientCapabilities, InitializeParams, RpcMessage, WorkspaceFolder};

use crate::lsp::config::{LSPInitOptions, LSPSettings};
use crate::lsp::utils::path_to_uri;

pub struct CangjieClient {
    client: Arc<Mutex<tokio_lsp::Client<tokio::process::ChildStdout, tokio::process::ChildStdin>>>,
    open_files: Arc<Mutex<HashMap<String, i32>>>,
    diagnostics: Arc<Mutex<HashMap<String, Vec<Value>>>>,
    initialized: Arc<std::sync::atomic::AtomicBool>,
    _child: Arc<Mutex<Option<tokio::process::Child>>>,
}

impl CangjieClient {
    pub async fn start(
        settings: &LSPSettings,
        init_options: &LSPInitOptions,
        env: &HashMap<String, String>,
    ) -> Result<Self> {
        let exe = settings.lsp_server_path();
        let args = settings.get_lsp_args();

        info!("Starting LSP server: {} {}", exe.display(), args.join(" "));

        let mut child = Command::new(&exe)
            .args(&args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .envs(env)
            .current_dir(&settings.workspace_path)
            .spawn()
            .context("Failed to start LSP server process")?;

        let stdin = child.stdin.take().context("No stdin")?;
        let stdout = child.stdout.take().context("No stdout")?;
        let stderr = child.stderr.take().context("No stderr")?;

        let lsp_client = tokio_lsp::Client::new(stdout, stdin);

        let client_arc = Arc::new(Mutex::new(lsp_client));
        let diagnostics = Arc::new(Mutex::new(HashMap::<String, Vec<Value>>::new()));
        let initialized = Arc::new(std::sync::atomic::AtomicBool::new(false));

        // Start stderr reader
        tokio::spawn(async move {
            Self::stderr_loop(stderr).await;
        });

        // Start background message receiver
        {
            let client_clone = client_arc.clone();
            let diagnostics_clone = diagnostics.clone();

            tokio::spawn(async move {
                loop {
                    let msg = {
                        let mut client = client_clone.lock().await;
                        client.receive_message().await
                    };
                    match msg {
                        Some(rpc_msg) => {
                            match &rpc_msg {
                                RpcMessage::Notification(notif) => {
                                    if notif.method == "textDocument/publishDiagnostics" {
                                        if let Some(ref params) = notif.params {
                                            if let Some(uri) =
                                                params.get("uri").and_then(|v| v.as_str())
                                            {
                                                let path = crate::lsp::utils::uri_to_path(uri)
                                                    .to_string_lossy()
                                                    .to_string();
                                                let diags = params
                                                    .get("diagnostics")
                                                    .and_then(|v| v.as_array())
                                                    .cloned()
                                                    .unwrap_or_default();
                                                diagnostics_clone.lock().await.insert(path, diags);
                                            }
                                        }
                                    }
                                }
                                RpcMessage::Request(req) => {
                                    let method = req.method.as_str();
                                    if method == "window/workDoneProgress/create"
                                        || method == "client/registerCapability"
                                        || method == "workspace/configuration"
                                    {
                                        let id = req.id.clone();
                                        let client = client_clone.lock().await;
                                        let _ =
                                            client.send_response(id, Some(Value::Null), None).await;
                                    }
                                }
                                RpcMessage::Response(_) => {
                                    // Responses are handled internally by tokio-lsp
                                }
                            }
                        }
                        None => break,
                    }
                }
            });
        }

        let result = Self {
            client: client_arc,
            open_files: Arc::new(Mutex::new(HashMap::new())),
            diagnostics,
            initialized,
            _child: Arc::new(Mutex::new(Some(child))),
        };

        // Initialize
        let root_uri = path_to_uri(&settings.workspace_path);
        let init_options_value = serde_json::to_value(init_options)?;

        let init_params = InitializeParams {
            process_id: Some(std::process::id()),
            root_uri: Some(root_uri.clone()),
            root_path: Some(settings.workspace_path.to_string_lossy().to_string()),
            workspace_folders: Some(vec![WorkspaceFolder {
                uri: root_uri,
                name: "workspace".to_string(),
            }]),
            initialization_options: Some(init_options_value),
            capabilities: ClientCapabilities::default(),
            trace: None,
            client_info: None,
            locale: None,
        };

        {
            let client = result.client.lock().await;
            let _init_result = client
                .initialize(init_params)
                .await
                .map_err(|e| anyhow::anyhow!("LSP initialization failed: {:?}", e))?;
            client
                .initialized()
                .await
                .map_err(|e| anyhow::anyhow!("LSP initialized notification failed: {:?}", e))?;
        }

        result
            .initialized
            .store(true, std::sync::atomic::Ordering::SeqCst);
        info!("LSP client initialized successfully");

        Ok(result)
    }

    pub fn is_initialized(&self) -> bool {
        self.initialized.load(std::sync::atomic::Ordering::SeqCst)
    }

    pub fn is_alive(&self) -> bool {
        self.is_initialized()
    }

    async fn stderr_loop(stderr: tokio::process::ChildStderr) {
        use tokio::io::AsyncBufReadExt;
        let reader = tokio::io::BufReader::new(stderr);
        let mut lines = reader.lines();
        loop {
            match lines.next_line().await {
                Ok(Some(line)) => {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        debug!("[LSP stderr] {}", trimmed);
                    }
                }
                Ok(None) => break,
                Err(_) => break,
            }
        }
    }

    // -- File management -----------------------------------------------------

    async fn ensure_open(&self, file_path: &str) -> Result<()> {
        let uri = path_to_uri(Path::new(file_path));
        let mut open_files = self.open_files.lock().await;

        if open_files.contains_key(&uri) {
            return Ok(());
        }

        let content = tokio::fs::read_to_string(file_path)
            .await
            .context("Failed to read file")?;

        {
            let client = self.client.lock().await;
            client
                .send_notification(
                    "textDocument/didOpen",
                    Some(json!({
                        "textDocument": {
                            "uri": uri,
                            "languageId": "cangjie",
                            "version": 1,
                            "text": content
                        }
                    })),
                )
                .await
                .map_err(|e| anyhow::anyhow!("didOpen failed: {:?}", e))?;
        }

        open_files.insert(uri, 1);
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        Ok(())
    }

    // -- LSP operations ------------------------------------------------------

    pub async fn definition(&self, file_path: &str, line: u32, character: u32) -> Result<Value> {
        self.ensure_open(file_path).await?;
        let uri = path_to_uri(Path::new(file_path));
        let client = self.client.lock().await;
        let resp = client
            .send_request(
                "textDocument/definition",
                Some(json!({
                    "textDocument": {"uri": uri},
                    "position": {"line": line, "character": character}
                })),
            )
            .await
            .map_err(|e| anyhow::anyhow!("definition request failed: {:?}", e))?;
        Ok(resp.result.unwrap_or(Value::Null))
    }

    pub async fn references(&self, file_path: &str, line: u32, character: u32) -> Result<Value> {
        self.ensure_open(file_path).await?;
        let uri = path_to_uri(Path::new(file_path));
        let client = self.client.lock().await;
        let resp = client
            .send_request(
                "textDocument/references",
                Some(json!({
                    "textDocument": {"uri": uri},
                    "position": {"line": line, "character": character},
                    "context": {"includeDeclaration": true}
                })),
            )
            .await
            .map_err(|e| anyhow::anyhow!("references request failed: {:?}", e))?;
        Ok(resp.result.unwrap_or(Value::Null))
    }

    pub async fn hover(&self, file_path: &str, line: u32, character: u32) -> Result<Value> {
        self.ensure_open(file_path).await?;
        let uri = path_to_uri(Path::new(file_path));
        let client = self.client.lock().await;
        let resp = client
            .send_request(
                "textDocument/hover",
                Some(json!({
                    "textDocument": {"uri": uri},
                    "position": {"line": line, "character": character}
                })),
            )
            .await
            .map_err(|e| anyhow::anyhow!("hover request failed: {:?}", e))?;
        Ok(resp.result.unwrap_or(Value::Null))
    }

    pub async fn completion(&self, file_path: &str, line: u32, character: u32) -> Result<Value> {
        self.ensure_open(file_path).await?;
        let uri = path_to_uri(Path::new(file_path));
        let client = self.client.lock().await;
        let resp = client
            .send_request(
                "textDocument/completion",
                Some(json!({
                    "textDocument": {"uri": uri},
                    "position": {"line": line, "character": character}
                })),
            )
            .await
            .map_err(|e| anyhow::anyhow!("completion request failed: {:?}", e))?;
        Ok(resp.result.unwrap_or(Value::Null))
    }

    pub async fn document_symbol(&self, file_path: &str) -> Result<Value> {
        self.ensure_open(file_path).await?;
        let uri = path_to_uri(Path::new(file_path));
        let client = self.client.lock().await;
        let resp = client
            .send_request(
                "textDocument/documentSymbol",
                Some(json!({
                    "textDocument": {"uri": uri}
                })),
            )
            .await
            .map_err(|e| anyhow::anyhow!("documentSymbol request failed: {:?}", e))?;
        Ok(resp.result.unwrap_or(Value::Null))
    }

    pub async fn get_diagnostics(&self, file_path: &str) -> Result<Vec<Value>> {
        self.ensure_open(file_path).await?;
        tokio::time::sleep(std::time::Duration::from_millis(1000)).await;

        let path_str = Path::new(file_path).to_string_lossy().to_string();
        let diags = self.diagnostics.lock().await;
        Ok(diags.get(&path_str).cloned().unwrap_or_default())
    }

    pub async fn shutdown(&self) -> Result<()> {
        let client = self.client.lock().await;
        let _ = client.send_request("shutdown", None).await;
        let _ = client.send_notification("exit", None).await;
        Ok(())
    }
}
