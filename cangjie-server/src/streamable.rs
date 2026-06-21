use std::sync::Arc;

use axum::http::header::{ACCEPT, CONTENT_TYPE};
use axum::http::{HeaderName, Method};
use rmcp::transport::common::http_header::{
    HEADER_LAST_EVENT_ID, HEADER_MCP_PROTOCOL_VERSION, HEADER_SESSION_ID,
};
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::tower::StreamableHttpService;

pub use rmcp::transport::streamable_http_server::tower::StreamableHttpServerConfig as McpServerConfig;
pub use tokio_util::sync::CancellationToken;

use crate::CangjieServer;

/// Non-safelisted response headers the transport writes (session id + negotiated
/// protocol version). A CORS layer must name them in
/// `Access-Control-Expose-Headers` or browser MCP clients can't read the session
/// id. From rmcp's constants to stay in sync with the transport.
pub fn mcp_exposed_headers() -> Vec<HeaderName> {
    [HEADER_SESSION_ID, HEADER_MCP_PROTOCOL_VERSION]
        .into_iter()
        .map(|name| HeaderName::from_bytes(name.as_bytes()).expect("rmcp header name is valid"))
        .collect()
}

/// Methods the transport accepts in stateful mode: `POST` (messages), `GET` (SSE
/// stream), `DELETE` (end session). `OPTIONS` preflight is answered by the CORS
/// layer, so it is not listed.
pub fn mcp_allowed_methods() -> Vec<Method> {
    vec![Method::GET, Method::POST, Method::DELETE]
}

/// Request headers browser MCP clients send, for `Access-Control-Allow-Headers`:
/// `accept`/`content-type` for JSON-vs-SSE negotiation and the body, plus the MCP
/// session, protocol-version, and SSE-resumption headers (from rmcp's constants).
pub fn mcp_allowed_headers() -> Vec<HeaderName> {
    let mut headers = vec![ACCEPT, CONTENT_TYPE];
    headers.extend(
        [
            HEADER_SESSION_ID,
            HEADER_MCP_PROTOCOL_VERSION,
            HEADER_LAST_EVENT_ID,
        ]
        .into_iter()
        .map(|name| HeaderName::from_bytes(name.as_bytes()).expect("rmcp header name is valid")),
    );
    headers
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
