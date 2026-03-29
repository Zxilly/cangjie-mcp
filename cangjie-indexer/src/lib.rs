pub mod api_client;
pub mod document;
pub mod embedding;
pub mod initializer;
pub mod repo;
pub mod rerank;
pub mod search;

#[cfg(test)]
pub(crate) mod testutil;

// Re-export core types for convenience
pub use cangjie_core::types::{
    DocData, DocMetadata, IndexMetadata, SearchMode, SearchResult, SearchResultMetadata, TextChunk,
};
