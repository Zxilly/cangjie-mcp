use std::sync::Arc;

use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::tower::StreamableHttpService;

pub use rmcp::transport::streamable_http_server::tower::StreamableHttpServerConfig as McpServerConfig;
pub use tokio_util::sync::CancellationToken;

use crate::CangjieServer;

/// Create a Streamable HTTP MCP service that can be mounted on an axum Router.
pub fn create_mcp_service(
    server: CangjieServer,
    config: McpServerConfig,
) -> StreamableHttpService<CangjieServer, LocalSessionManager> {
    StreamableHttpService::new(
        move || Ok(server.clone()),
        Arc::new(LocalSessionManager::default()),
        config,
    )
}
