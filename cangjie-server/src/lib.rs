pub mod mcp_handler;

#[cfg(feature = "http")]
pub mod http;

#[cfg(feature = "streamable-http")]
pub mod streamable;

#[cfg(feature = "lsp")]
pub mod lsp_pool;

pub mod lsp_tools;

pub use mcp_handler::CangjieServer;
pub use rmcp::handler::server::wrapper::Parameters;
