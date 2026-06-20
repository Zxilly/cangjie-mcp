use std::sync::Arc;

use axum::http::HeaderName;
use rmcp::transport::common::http_header::{HEADER_MCP_PROTOCOL_VERSION, HEADER_SESSION_ID};
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::tower::StreamableHttpService;

pub use rmcp::transport::streamable_http_server::tower::StreamableHttpServerConfig as McpServerConfig;
pub use tokio_util::sync::CancellationToken;

use crate::CangjieServer;

/// Response headers the Streamable HTTP MCP transport writes onto responses (the
/// session id and the negotiated protocol version). They are non-safelisted, so
/// any CORS layer in front of the service must name them in
/// `Access-Control-Expose-Headers`, otherwise browser MCP clients cannot read
/// the session id and fail to establish a session.
///
/// The names are taken straight from rmcp's own constants so they stay in sync
/// with the transport rather than being hardcoded at the call site.
pub fn mcp_exposed_headers() -> Vec<HeaderName> {
    [HEADER_SESSION_ID, HEADER_MCP_PROTOCOL_VERSION]
        .into_iter()
        .map(|name| HeaderName::from_bytes(name.as_bytes()).expect("rmcp header name is valid"))
        .collect()
}

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
