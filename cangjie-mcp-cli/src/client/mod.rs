use anyhow::{Context, Result};
use rmcp::model::{CallToolRequestParams, CallToolResult, ClientInfo};
use rmcp::ServiceExt;

use crate::daemon::ipc::ipc_connect;

pub async fn call_tool(params: CallToolRequestParams) -> Result<CallToolResult> {
    let stream = ipc_connect().await.context("failed to connect to daemon")?;

    let client_info = ClientInfo::new(
        Default::default(),
        rmcp::model::Implementation::new("cangjie-mcp-cli", env!("CARGO_PKG_VERSION")),
    );

    let service = client_info
        .serve(stream)
        .await
        .map_err(|e| anyhow::anyhow!("MCP handshake failed: {e}"))?;

    let result = service
        .peer()
        .call_tool(params)
        .await
        .map_err(|e| anyhow::anyhow!("tool call failed: {e}"))?;

    if let Err(e) = service.cancel().await {
        tracing::debug!("MCP service close: {e}");
    }

    Ok(result)
}
