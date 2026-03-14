use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use jsonrpsee::core::client::{ReceivedMessage, TransportReceiverT, TransportSenderT};
use serde_json::{json, value::RawValue, Value};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, Mutex, Notify};
use tracing::{debug, error, info, warn};

const CONTENT_LENGTH: &str = "Content-Length";

// -- Transport error ---------------------------------------------------------

#[derive(Debug)]
pub(crate) enum LspTransportError {
    ChannelClosed,
    InvalidJson(String),
    Eof,
}

impl std::fmt::Display for LspTransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ChannelClosed => f.write_str("LSP channel closed"),
            Self::InvalidJson(e) => write!(f, "invalid JSON from LSP: {e}"),
            Self::Eof => f.write_str("LSP connection EOF"),
        }
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
        debug!("[LSP queue] {}", msg);
        self.outbound_tx
            .send(msg)
            .map_err(|_| LspTransportError::ChannelClosed)
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
    pub(crate) diagnostic_versions: Arc<Mutex<HashMap<String, u64>>>,
    pub(crate) diagnostics_notify: Arc<Notify>,
}

impl TransportReceiverT for LspReceiver {
    type Error = LspTransportError;

    async fn receive(&mut self) -> std::result::Result<ReceivedMessage, Self::Error> {
        loop {
            let body = self
                .incoming_rx
                .recv()
                .await
                .ok_or(LspTransportError::Eof)?;

            // Parse once into Value; discriminate by presence of "id" and "method"
            // following the JSON-RPC 2.0 spec (same approach as async-lsp's
            // `#[serde(untagged)] enum Message`).
            let msg: Value = serde_json::from_str(&body)
                .map_err(|e| LspTransportError::InvalidJson(e.to_string()))?;

            info!("[LSP recv] {}", body);

            let has_id = msg.get("id").is_some();
            let method = msg.get("method").and_then(|m| m.as_str());

            match (has_id, method) {
                // Request from server (has both id and method)
                (true, Some(m)) => {
                    info!("[LSP] Handling server request: {}", m);
                    self.handle_server_request(&msg, m);
                }
                // Response to our request (has id, no method)
                (true, None) => {
                    return Ok(ReceivedMessage::Text(body));
                }
                // Notification from server (no id, has method)
                (false, Some(m)) => {
                    info!("[LSP] Handling notification: {}", m);
                    self.handle_notification(&msg, m).await;
                }
                // Invalid JSON-RPC message (no id, no method)
                (false, None) => {
                    warn!("[LSP] Received message with neither id nor method, forwarding");
                    return Ok(ReceivedMessage::Text(body));
                }
            }
        }
    }
}

impl LspReceiver {
    fn handle_server_request(&self, msg: &Value, method: &str) {
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

    async fn handle_notification(&self, msg: &Value, method: &str) {
        if method == "textDocument/publishDiagnostics" {
            if let Some(params) = msg.get("params") {
                if let Some(uri) = params.get("uri").and_then(|v| v.as_str()) {
                    let path = crate::utils::uri_to_path(uri).to_string_lossy().to_string();
                    let diags = params
                        .get("diagnostics")
                        .and_then(|v| v.as_array())
                        .cloned()
                        .unwrap_or_default();
                    self.diagnostics.lock().await.insert(path.clone(), diags);
                    let mut versions = self.diagnostic_versions.lock().await;
                    *versions.entry(path).or_insert(0) += 1;
                    self.diagnostics_notify.notify_waiters();
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
///
/// Header parsing follows the LSP base protocol strictly (inspired by async-lsp):
/// headers are `Name: Value\r\n` terminated by a blank `\r\n` line.
/// Body bytes are read via `read_exact` and forwarded as a UTF-8 string.
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

        // Use from_utf8_lossy-free path: LSP mandates UTF-8.
        // Validate + convert in one step via from_utf8.
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

/// Read LSP headers until the blank `\r\n` separator, extract `Content-Length`.
///
/// Follows the strict LSP base protocol format (aligned with async-lsp):
/// - Each header line ends with `\r\n`
/// - Header name and value are separated by `: ` (colon + space)
/// - Header names are matched case-insensitively
async fn read_content_length<R: tokio::io::AsyncBufRead + Unpin>(
    reader: &mut R,
    header_buf: &mut String,
    running: &AtomicBool,
) -> Result<usize, ()> {
    let mut content_length: Option<usize> = None;

    loop {
        header_buf.clear();
        let bytes_read = reader.read_line(header_buf).await.map_err(|e| {
            log_stream_close(running, &e.to_string());
        })?;

        if bytes_read == 0 {
            log_stream_close(running, "EOF");
            return Err(());
        }

        // Strip line ending: prefer strict "\r\n", tolerate bare "\n"
        let line = header_buf
            .strip_suffix("\r\n")
            .or_else(|| header_buf.strip_suffix('\n'))
            .unwrap_or(header_buf);

        if line.is_empty() {
            break;
        }

        if let Some((name, value)) = line.split_once(": ") {
            if name.eq_ignore_ascii_case(CONTENT_LENGTH) {
                content_length = Some(value.trim().parse().map_err(|_| {
                    error!("LSP invalid Content-Length value: {value:?}");
                })?);
            }
        } else {
            debug!("LSP ignoring malformed header: {:?}", line);
        }
    }

    content_length.ok_or_else(|| {
        error!("LSP message missing Content-Length header");
    })
}

fn log_stream_close(running: &AtomicBool, detail: &str) {
    if running.load(Ordering::SeqCst) {
        error!(
            "LSP stdout stream closed unexpectedly ({detail}) — \
             the LSP server process may have crashed. \
             LSP tools will be unavailable."
        );
    } else {
        debug!("LSP stdout stream closed during shutdown");
    }
}

pub(crate) async fn stdin_task(
    mut stdin: tokio::process::ChildStdin,
    mut outbound_rx: mpsc::UnboundedReceiver<String>,
    running: Arc<AtomicBool>,
) {
    while let Some(message) = outbound_rx.recv().await {
        info!("[LSP write] {}", message);
        // Write header and body separately to avoid copying the entire body
        // into a new String just to prepend a small header.
        let header = format!("{CONTENT_LENGTH}: {}\r\n\r\n", message.len());
        if let Err(e) = stdin.write_all(header.as_bytes()).await {
            if running.load(Ordering::SeqCst) {
                error!("Failed to write to LSP stdin: {}", e);
            }
            break;
        }
        if let Err(e) = stdin.write_all(message.as_bytes()).await {
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
