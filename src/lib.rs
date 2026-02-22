pub mod config;
pub mod error;
pub mod indexer;
pub mod lsp;
pub mod prompts;
pub mod repo;
pub mod server;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
