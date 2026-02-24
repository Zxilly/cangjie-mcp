use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::{Context, Result};
use jsonrpsee::core::client::ClientT;
use serde_json::Value;
use tokio::process::Command;
use tokio::sync::{mpsc, Mutex};
use tracing::{debug, info, warn};

use crate::lsp::config::{LSPInitOptions, LSPSettings};
use crate::lsp::transport::{
    process_monitor, stderr_task, stdin_task, stdout_reader_task, LspParams, LspReceiver, LspSender,
};
use crate::lsp::types::{
    CallHierarchyIncomingCallsParams, CallHierarchyOutgoingCallsParams, CallHierarchyPrepareParams,
    ClientCapabilities, ClientInfo, CompletionParams, DidOpenTextDocumentParams,
    DocumentSymbolParams, GotoDefinitionParams, HoverParams, InitializeParams, InitializedParams,
    Position, ReferenceContext, ReferenceParams, RenameParams, SignatureHelpParams,
    TextDocumentIdentifier, TextDocumentItem, TextDocumentPositionParams, TraceValue,
    TypeHierarchyPrepareParams, TypeHierarchySubtypesParams, TypeHierarchySupertypesParams, Uri,
    WorkDoneProgressParams, WorkspaceFolder, WorkspaceSymbolParams,
};
use crate::lsp::utils::path_to_uri;

const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

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

// -- Client capabilities -----------------------------------------------------

fn build_client_capabilities() -> ClientCapabilities {
    let caps_json = serde_json::json!({
        "workspace": {
            "applyEdit": true,
            "workspaceEdit": {
                "documentChanges": true,
                "resourceOperations": ["create", "rename", "delete"],
                "failureHandling": "textOnlyTransactional",
                "normalizesLineEndings": true,
                "changeAnnotationSupport": {
                    "groupsOnLabel": true
                }
            },
            "configuration": true,
            "didChangeWatchedFiles": {
                "dynamicRegistration": true,
                "relativePatternSupport": true
            },
            "symbol": {
                "dynamicRegistration": true,
                "symbolKind": {
                    "valueSet": [1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26]
                },
                "tagSupport": {
                    "valueSet": [1]
                },
                "resolveSupport": {
                    "properties": ["location.range"]
                }
            },
            "codeLens": {
                "refreshSupport": true
            },
            "executeCommand": {
                "dynamicRegistration": true
            },
            "didChangeConfiguration": {
                "dynamicRegistration": true
            },
            "workspaceFolders": true,
            "semanticTokens": {
                "refreshSupport": true
            },
            "fileOperations": {
                "dynamicRegistration": true,
                "didCreate": true,
                "didRename": true,
                "didDelete": true,
                "willCreate": true,
                "willRename": true,
                "willDelete": true
            },
            "inlineValue": {
                "refreshSupport": true
            },
            "inlayHint": {
                "refreshSupport": true
            },
            "diagnostics": {
                "refreshSupport": true
            }
        },
        "textDocument": {
            "publishDiagnostics": {
                "relatedInformation": true,
                "versionSupport": false,
                "tagSupport": {
                    "valueSet": [1, 2]
                },
                "codeDescriptionSupport": true,
                "dataSupport": true
            },
            "synchronization": {
                "dynamicRegistration": true,
                "willSave": true,
                "willSaveWaitUntil": true,
                "didSave": true
            },
            "completion": {
                "dynamicRegistration": true,
                "contextSupport": true,
                "completionItem": {
                    "snippetSupport": true,
                    "commitCharactersSupport": true,
                    "documentationFormat": ["markdown", "plaintext"],
                    "deprecatedSupport": true,
                    "preselectSupport": true,
                    "tagSupport": {
                        "valueSet": [1]
                    },
                    "insertReplaceSupport": true,
                    "resolveSupport": {
                        "properties": ["documentation", "detail", "additionalTextEdits"]
                    },
                    "insertTextModeSupport": {
                        "valueSet": [1, 2]
                    },
                    "labelDetailsSupport": true
                },
                "insertTextMode": 2,
                "completionItemKind": {
                    "valueSet": [1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25]
                },
                "completionList": {
                    "itemDefaults": ["commitCharacters", "editRange", "insertTextFormat", "insertTextMode"]
                }
            },
            "hover": {
                "dynamicRegistration": true,
                "contentFormat": ["markdown", "plaintext"]
            },
            "signatureHelp": {
                "dynamicRegistration": true,
                "signatureInformation": {
                    "documentationFormat": ["markdown", "plaintext"],
                    "parameterInformation": {
                        "labelOffsetSupport": true
                    },
                    "activeParameterSupport": true
                },
                "contextSupport": true
            },
            "definition": {
                "dynamicRegistration": true,
                "linkSupport": true
            },
            "references": {
                "dynamicRegistration": true
            },
            "documentHighlight": {
                "dynamicRegistration": true
            },
            "documentSymbol": {
                "dynamicRegistration": true,
                "symbolKind": {
                    "valueSet": [1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26]
                },
                "hierarchicalDocumentSymbolSupport": true,
                "tagSupport": {
                    "valueSet": [1]
                },
                "labelSupport": true
            },
            "codeAction": {
                "dynamicRegistration": true,
                "isPreferredSupport": true,
                "disabledSupport": true,
                "dataSupport": true,
                "resolveSupport": {
                    "properties": ["edit"]
                },
                "codeActionLiteralSupport": {
                    "codeActionKind": {
                        "valueSet": [
                            "",
                            "quickfix",
                            "refactor",
                            "refactor.extract",
                            "refactor.inline",
                            "refactor.rewrite",
                            "source",
                            "source.organizeImports"
                        ]
                    }
                },
                "honorsChangeAnnotations": false
            },
            "codeLens": {
                "dynamicRegistration": true
            },
            "formatting": {
                "dynamicRegistration": true
            },
            "rangeFormatting": {
                "dynamicRegistration": true
            },
            "onTypeFormatting": {
                "dynamicRegistration": true
            },
            "rename": {
                "dynamicRegistration": true,
                "prepareSupport": true,
                "prepareSupportDefaultBehavior": 1,
                "honorsChangeAnnotations": true
            },
            "documentLink": {
                "dynamicRegistration": true,
                "tooltipSupport": true
            },
            "typeDefinition": {
                "dynamicRegistration": true,
                "linkSupport": true
            },
            "implementation": {
                "dynamicRegistration": true,
                "linkSupport": true
            },
            "colorProvider": {
                "dynamicRegistration": true
            },
            "foldingRange": {
                "dynamicRegistration": true,
                "rangeLimit": 5000,
                "lineFoldingOnly": true,
                "foldingRangeKind": {
                    "valueSet": ["comment", "imports", "region"]
                },
                "foldingRange": {
                    "collapsedText": false
                }
            },
            "declaration": {
                "dynamicRegistration": true,
                "linkSupport": true
            },
            "selectionRange": {
                "dynamicRegistration": true
            },
            "callHierarchy": {
                "dynamicRegistration": true
            },
            "semanticTokens": {
                "dynamicRegistration": true,
                "tokenTypes": [
                    "namespace", "type", "class", "enum", "interface", "struct",
                    "typeParameter", "parameter", "variable", "property", "enumMember",
                    "event", "function", "method", "macro", "keyword", "modifier",
                    "comment", "string", "number", "regexp", "operator", "decorator"
                ],
                "tokenModifiers": [
                    "declaration", "definition", "readonly", "static", "deprecated",
                    "abstract", "async", "modification", "documentation", "defaultLibrary"
                ],
                "formats": ["relative"],
                "requests": {
                    "range": true,
                    "full": {
                        "delta": true
                    }
                },
                "multilineTokenSupport": false,
                "overlappingTokenSupport": false,
                "serverCancelSupport": true,
                "augmentsSyntaxTokens": true
            },
            "linkedEditingRange": {
                "dynamicRegistration": true
            },
            "typeHierarchy": {
                "dynamicRegistration": true
            },
            "inlineValue": {
                "dynamicRegistration": true
            },
            "inlayHint": {
                "dynamicRegistration": true,
                "resolveSupport": {
                    "properties": [
                        "tooltip", "textEdits", "label.tooltip",
                        "label.location", "label.command"
                    ]
                }
            },
            "diagnostic": {
                "dynamicRegistration": true,
                "relatedDocumentSupport": false
            }
        },
        "window": {
            "showMessage": {
                "messageActionItem": {
                    "additionalPropertiesSupport": true
                }
            },
            "showDocument": {
                "support": true
            },
            "workDoneProgress": true
        },
        "general": {
            "staleRequestSupport": {
                "cancel": true,
                "retryOnContentModified": [
                    "textDocument/semanticTokens/full",
                    "textDocument/semanticTokens/range",
                    "textDocument/semanticTokens/full/delta"
                ]
            },
            "regularExpressions": {
                "engine": "ECMAScript",
                "version": "ES2020"
            },
            "markdown": {
                "parser": "marked",
                "version": "1.1.0"
            },
            "positionEncodings": ["utf-16"]
        },
        "notebookDocument": {
            "synchronization": {
                "dynamicRegistration": true,
                "executionSummarySupport": true
            }
        }
    });

    serde_json::from_value(caps_json).unwrap_or_default()
}

// -- Shell wrapper -----------------------------------------------------------

fn build_shell_command(settings: &LSPSettings, require_path: &str) -> Result<Command> {
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

/// Captures environment variables after sourcing envsetup.ps1 via a separate
/// PowerShell process. The resulting HashMap contains the full environment
/// (inherited + SDK modifications) and can be passed to `env_clear() + envs()`
/// on the LSP server Command.
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

// -- Client ------------------------------------------------------------------

pub struct CangjieClient {
    client: jsonrpsee::core::client::Client,
    open_files: Mutex<HashMap<String, i32>>,
    diagnostics: Arc<Mutex<HashMap<String, Vec<Value>>>>,
    initialized: AtomicBool,
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
        let running = Arc::new(AtomicBool::new(true));

        // Stdout reader task: continuously reads from LSP stdout into unbounded buffer
        tokio::spawn(stdout_reader_task(stdout, incoming_tx, running.clone()));

        // Build jsonrpsee transport
        let sender = LspSender {
            outbound_tx: outbound_tx.clone(),
        };
        let receiver = LspReceiver {
            incoming_rx,
            outbound_tx: outbound_tx.clone(),
            diagnostics: diagnostics.clone(),
        };

        let rpc_client = jsonrpsee::core::client::ClientBuilder::default()
            .request_timeout(REQUEST_TIMEOUT)
            .build_with_tokio(sender, receiver);

        // Stdin writer task: reads from channel, writes Content-Length framed messages
        tokio::spawn(stdin_task(stdin, outbound_rx, running.clone()));

        // Stderr reader task
        tokio::spawn(stderr_task(stderr));

        // Process monitor task
        tokio::spawn(process_monitor(child, running.clone()));

        let client = Self {
            client: rpc_client,
            open_files: Mutex::new(HashMap::new()),
            diagnostics,
            initialized: AtomicBool::new(false),
            running,
        };

        // LSP initialize handshake
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
        let _: Value = self
            .client
            .request("initialize", LspParams::new(&init_params)?)
            .await
            .map_err(|e| anyhow::anyhow!("LSP initialization failed: {e}"))?;

        debug!("[LSP] initialized");
        self.client
            .notification("initialized", LspParams::new(&InitializedParams {})?)
            .await
            .map_err(|e| anyhow::anyhow!("LSP initialized notification failed: {e}"))?;

        self.initialized.store(true, Ordering::SeqCst);
        info!("LSP client initialized successfully");
        Ok(())
    }

    pub fn is_initialized(&self) -> bool {
        self.initialized.load(Ordering::SeqCst)
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst) && self.client.is_connected()
    }

    pub fn is_alive(&self) -> bool {
        self.is_initialized() && self.is_running()
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

    async fn ensure_open(&self, file_path: &str) -> Result<()> {
        let uri_str = path_to_uri(Path::new(file_path));

        {
            let open_files = self.open_files.lock().await;
            if open_files.contains_key(&uri_str) {
                return Ok(());
            }
        }

        let content = tokio::fs::read_to_string(file_path)
            .await
            .context("Failed to read file")?;

        let params = DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: parse_uri(&uri_str)?,
                language_id: "Cangjie".to_string(),
                version: 1,
                text: content,
            },
        };

        debug!("[LSP] textDocument/didOpen: {}", uri_str);
        self.notify("textDocument/didOpen", &params).await?;

        self.open_files.lock().await.insert(uri_str, 1);
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

    pub async fn completion(&self, file_path: &str, line: u32, character: u32) -> Result<Value> {
        self.ensure_open(file_path).await?;
        let uri = parse_uri(&path_to_uri(Path::new(file_path)))?;
        self.document_request(
            "textDocument/completion",
            &CompletionParams {
                text_document_position: make_td_position(uri.clone(), line, character),
                context: None,
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: Default::default(),
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

    pub async fn rename(
        &self,
        file_path: &str,
        line: u32,
        character: u32,
        new_name: &str,
    ) -> Result<Value> {
        self.ensure_open(file_path).await?;
        let uri = parse_uri(&path_to_uri(Path::new(file_path)))?;
        let params = RenameParams {
            text_document_position: make_td_position(uri.clone(), line, character),
            new_name: new_name.to_string(),
            work_done_progress_params: WorkDoneProgressParams::default(),
        };
        self.document_request("textDocument/rename", &params, &uri)
            .await
    }

    pub async fn signature_help(
        &self,
        file_path: &str,
        line: u32,
        character: u32,
    ) -> Result<Value> {
        self.ensure_open(file_path).await?;
        let uri = parse_uri(&path_to_uri(Path::new(file_path)))?;
        let params = SignatureHelpParams {
            text_document_position_params: make_td_position(uri.clone(), line, character),
            work_done_progress_params: WorkDoneProgressParams::default(),
            context: None,
        };
        self.document_request("textDocument/signatureHelp", &params, &uri)
            .await
    }

    pub async fn get_diagnostics(&self, file_path: &str) -> Result<Vec<Value>> {
        self.ensure_open(file_path).await?;
        tokio::time::sleep(std::time::Duration::from_millis(1000)).await;

        let path_str = Path::new(file_path).to_string_lossy().to_string();
        let diags = self.diagnostics.lock().await;
        Ok(diags.get(&path_str).cloned().unwrap_or_default())
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

        Ok(())
    }
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
            // No PATH prepend when require_path is empty
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
