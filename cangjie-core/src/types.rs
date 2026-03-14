use serde::{Deserialize, Serialize};

// -- Shared types ------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SearchResultMetadata {
    pub file_path: String,
    pub category: String,
    pub topic: String,
    pub title: String,
    pub has_code: bool,
    #[serde(default)]
    pub chunk_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub text: String,
    pub score: f64,
    pub metadata: SearchResultMetadata,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SearchMode {
    #[default]
    Bm25,
    Hybrid,
}

impl std::fmt::Display for SearchMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bm25 => write!(f, "bm25"),
            Self::Hybrid => write!(f, "hybrid"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexMetadata {
    pub version: String,
    pub lang: String,
    pub embedding_model: String,
    pub document_count: usize,
    #[serde(default)]
    pub search_mode: SearchMode,
}

/// Lightweight document container (no framework dependency).
#[derive(Debug, Clone)]
pub struct DocData {
    pub text: String,
    pub metadata: DocMetadata,
    pub doc_id: String,
}

#[derive(Debug, Clone, Default)]
pub struct DocMetadata {
    pub file_path: String,
    pub category: String,
    pub topic: String,
    pub title: String,
    pub code_block_count: usize,
    pub has_code: bool,
    pub chunk_id: String,
}

/// A text chunk produced by the chunker with its metadata.
#[derive(Debug, Clone)]
pub struct TextChunk {
    pub text: String,
    pub metadata: DocMetadata,
}
