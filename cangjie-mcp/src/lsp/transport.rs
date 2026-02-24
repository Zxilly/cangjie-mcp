use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use jsonrpsee::core::client::{ReceivedMessage, TransportReceiverT, TransportSenderT};
use serde_json::{json, value::RawValue, Value};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, Mutex};
use tracing::{debug, error, info, warn};

pub(crate) const CONTENT_LENGTH_HEADER: &str = "Content-Length: ";

// -- Transport error ---------------------------------------------------------

#[derive(Debug)]
pub(crate) struct LspTransportError(String);

impl std::fmt::Display for LspTransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for LspTransportError {}

// -- Transport sender --------------------------------------------------------

/// Writes JSON-RPC messages to an outbound channel (consumed by the stdin writer task).
pub(crate) struct LspSender {
    pub(crate) outbound_tx: mpsc::UnboundedSender<String>,
}

impl TransportSenderT for LspSender {
    type Error = LspTransportError;

    async fn send(&mut self, msg: String) -> std::result::Result<(), Self::Error> {
        debug!("[LSP send] {}", msg);
        self.outbound_tx
            .send(msg)
            .map_err(|_| LspTransportError("LSP outbound channel closed".into()))
    }

    async fn close(&mut self) -> std::result::Result<(), Self::Error> {
        Ok(())
    }
}

// -- Transport receiver ------------------------------------------------------

/// Reads JSON-RPC messages from an unbounded buffer (fed by stdout_reader_task).
///
/// Intercepts server notifications (diagnostics) and server-initiated requests
/// (workDoneProgress, registerCapability, configuration), handling them
/// internally. Only actual responses are forwarded to the jsonrpsee client.
pub(crate) struct LspReceiver {
    pub(crate) incoming_rx: mpsc::UnboundedReceiver<String>,
    pub(crate) outbound_tx: mpsc::UnboundedSender<String>,
    pub(crate) diagnostics: Arc<Mutex<HashMap<String, Vec<Value>>>>,
}

impl TransportReceiverT for LspReceiver {
    type Error = LspTransportError;

    async fn receive(&mut self) -> std::result::Result<ReceivedMessage, Self::Error> {
        loop {
            let body = self
                .incoming_rx
                .recv()
                .await
                .ok_or_else(|| LspTransportError("LSP connection closed".into()))?;

            let msg: Value = serde_json::from_str(&body)
                .map_err(|e| LspTransportError(format!("Invalid JSON from LSP: {e}")))?;

            info!("[LSP recv] {}", body);

            if let Some(_id) = msg.get("id") {
                if msg.get("method").is_some() {
                    // Server → client request: handle internally, send response
                    info!("[LSP] Handling server request: {}", msg["method"]);
                    self.handle_server_request(&msg);
                    continue;
                }
                // Response to our request: forward to jsonrpsee
                info!("[LSP] Forwarding response id={}", _id);
                return Ok(ReceivedMessage::Text(body));
            }

            if msg.get("method").is_some() {
                // Server notification: handle internally
                info!("[LSP] Handling notification: {}", msg["method"]);
                self.handle_notification(&msg).await;
                continue;
            }

            // Unknown message type — forward to jsonrpsee
            return Ok(ReceivedMessage::Text(body));
        }
    }
}

impl LspReceiver {
    fn handle_server_request(&self, msg: &Value) {
        let method = msg["method"].as_str().unwrap_or("");
        let id = &msg["id"];

        let result = match method {
            "window/workDoneProgress/create" | "client/registerCapability" => Value::Null,
            "workspace/configuration" => {
                let items_count = msg
                    .get("params")
                    .and_then(|p| p.get("items"))
                    .and_then(|i| i.as_array())
                    .map(|a| a.len())
                    .unwrap_or(1);
                Value::Array(vec![json!({}); items_count])
            }
            _ => {
                debug!("[LSP] Unhandled server request: {}", method);
                Value::Null
            }
        };

        let response = json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": result
        });

        if let Ok(serialized) = serde_json::to_string(&response) {
            let _ = self.outbound_tx.send(serialized);
        }
    }

    async fn handle_notification(&self, msg: &Value) {
        let method = msg["method"].as_str().unwrap_or("");

        if method == "textDocument/publishDiagnostics" {
            if let Some(params) = msg.get("params") {
                if let Some(uri) = params.get("uri").and_then(|v| v.as_str()) {
                    let path = crate::lsp::utils::uri_to_path(uri)
                        .to_string_lossy()
                        .to_string();
                    let diags = params
                        .get("diagnostics")
                        .and_then(|v| v.as_array())
                        .cloned()
                        .unwrap_or_default();
                    self.diagnostics.lock().await.insert(path, diags);
                }
            }
        }
    }
}

// -- RPC params adapter ------------------------------------------------------

/// Wraps a pre-serialized value for use with jsonrpsee's `ClientT` methods.
pub(crate) struct LspParams(Option<Box<RawValue>>);

impl jsonrpsee::core::traits::ToRpcParams for LspParams {
    fn to_rpc_params(self) -> std::result::Result<Option<Box<RawValue>>, serde_json::Error> {
        Ok(self.0)
    }
}

impl LspParams {
    pub(crate) fn new<T: serde::Serialize>(
        params: &T,
    ) -> std::result::Result<Self, serde_json::Error> {
        Ok(LspParams(Some(serde_json::value::to_raw_value(params)?)))
    }

    pub(crate) fn empty() -> Self {
        LspParams(None)
    }
}

// -- Background tasks --------------------------------------------------------

/// Continuously reads Content-Length framed JSON-RPC messages from LSP stdout
/// and pushes them into an unbounded channel. This allows the LSP server to
/// write without blocking on pipe buffer limits; receive() consumes on demand.
pub(crate) async fn stdout_reader_task(
    stdout: tokio::process::ChildStdout,
    incoming_tx: mpsc::UnboundedSender<String>,
    running: Arc<AtomicBool>,
) {
    let mut reader = BufReader::new(stdout);
    let mut header_buf = String::new();

    loop {
        let content_length = match read_content_length(&mut reader, &mut header_buf, &running).await
        {
            Ok(len) => len,
            Err(_) => break,
        };

        let mut body_buf = vec![0u8; content_length];
        if let Err(e) = reader.read_exact(&mut body_buf).await {
            if running.load(Ordering::SeqCst) {
                error!("LSP body read error: {}", e);
            }
            break;
        }

        let body = match String::from_utf8(body_buf) {
            Ok(s) => s,
            Err(e) => {
                error!("LSP non-UTF8 message: {}", e);
                continue;
            }
        };

        if incoming_tx.send(body).is_err() {
            debug!("LSP incoming channel closed, stdout reader exiting");
            break;
        }
    }
    debug!("LSP stdout reader task exited");
}

async fn read_content_length<R: tokio::io::AsyncBufRead + Unpin>(
    reader: &mut R,
    header_buf: &mut String,
    running: &AtomicBool,
) -> Result<usize, ()> {
    let mut content_length: Option<usize> = None;

    loop {
        header_buf.clear();
        let bytes_read = reader.read_line(header_buf).await.map_err(|_e| {
            if running.load(Ordering::SeqCst) {
                error!(
                    "LSP stdout stream closed unexpectedly — \
                     the LSP server process may have crashed. \
                     LSP tools will be unavailable."
                );
            } else {
                debug!("LSP stdout stream closed during shutdown");
            }
        })?;

        if bytes_read == 0 {
            if running.load(Ordering::SeqCst) {
                error!(
                    "LSP stdout stream closed unexpectedly — \
                     the LSP server process may have crashed. \
                     LSP tools will be unavailable."
                );
            } else {
                debug!("LSP stdout stream closed during shutdown");
            }
            return Err(());
        }

        let line = header_buf.trim();
        if line.is_empty() {
            break;
        }

        if let Some(len_str) = line.strip_prefix(CONTENT_LENGTH_HEADER) {
            content_length = Some(len_str.trim().parse().map_err(|_| ())?);
        }
    }

    content_length.ok_or(())
}

pub(crate) async fn stdin_task(
    mut stdin: tokio::process::ChildStdin,
    mut outbound_rx: mpsc::UnboundedReceiver<String>,
    running: Arc<AtomicBool>,
) {
    while let Some(message) = outbound_rx.recv().await {
        // Batch all pending messages into a single write so the LSP server
        // receives them in the same pipe-buffer read.  The Cangjie LSP
        // server only calls ReadyForDiagnostics when it can peek ahead at
        // the next message during the ArkASTWorker processing loop; writing
        // messages one-at-a-time with a flush in-between prevents the server
        // from seeing the next message when it needs to.
        info!("[LSP send] {}", message);
        let mut batch = format!(
            "{CONTENT_LENGTH_HEADER}{}\r\n\r\n{}",
            message.len(),
            message
        );

        while let Ok(message) = outbound_rx.try_recv() {
            info!("[LSP send] {}", message);
            batch.push_str(&format!(
                "{CONTENT_LENGTH_HEADER}{}\r\n\r\n{}",
                message.len(),
                message
            ));
        }

        if let Err(e) = stdin.write_all(batch.as_bytes()).await {
            if running.load(Ordering::SeqCst) {
                error!("Failed to write to LSP stdin: {}", e);
            }
            break;
        }
        if let Err(e) = stdin.flush().await {
            if running.load(Ordering::SeqCst) {
                error!("Failed to flush LSP stdin: {}", e);
            }
            break;
        }
    }
    debug!("LSP stdin task exited");
}

pub(crate) async fn stderr_task(stderr: tokio::process::ChildStderr) {
    let reader = BufReader::new(stderr);
    let mut lines = reader.lines();
    loop {
        match lines.next_line().await {
            Ok(Some(line)) => {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    warn!("[LSP stderr] {}", trimmed);
                }
            }
            Ok(None) => {
                debug!("LSP stderr stream closed");
                break;
            }
            Err(e) => {
                error!("LSP stderr read error: {}", e);
                break;
            }
        }
    }
}

pub(crate) async fn process_monitor(mut child: tokio::process::Child, running: Arc<AtomicBool>) {
    match child.wait().await {
        Ok(status) if status.success() => {
            running.store(false, Ordering::SeqCst);
            info!("LSP server process exited normally: {}", status);
        }
        Ok(status) => {
            running.store(false, Ordering::SeqCst);
            error!(
                "LSP server process exited with error: {}. \
                 LSP tools will be unavailable.",
                status
            );
        }
        Err(e) => {
            running.store(false, Ordering::SeqCst);
            error!(
                "Failed to wait for LSP server process: {}. \
                 LSP tools will be unavailable.",
                e
            );
        }
    }
}
