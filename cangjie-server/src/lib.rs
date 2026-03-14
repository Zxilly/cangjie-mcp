pub mod mcp_handler;

#[cfg(feature = "http")]
pub mod http;

pub mod lsp_tools;

pub use mcp_handler::CangjieServer;
pub use rmcp::handler::server::wrapper::Parameters;
