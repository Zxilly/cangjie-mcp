pub mod document;
pub mod embedding;
pub mod initializer;
pub mod rerank;
pub mod search;

use std::time::Duration;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::config::Settings;

// -- Shared types ------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SearchResultMetadata {
    pub file_path: String,
    pub category: String,
    pub topic: String,
    pub title: String,
    pub has_code: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub text: String,
    pub score: f64,
    pub metadata: SearchResultMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexMetadata {
    pub version: String,
    pub lang: String,
    pub embedding_model: String,
    pub document_count: usize,
    #[serde(default = "default_search_mode")]
    pub search_mode: String,
}

fn default_search_mode() -> String {
    "bm25".to_string()
}

/// Build a shared HTTP client optimized for external API calls.
pub(crate) fn build_http_client(settings: &Settings, timeout: Duration) -> Result<reqwest::Client> {
    let mut builder = reqwest::Client::builder()
        .timeout(timeout)
        .pool_idle_timeout(Duration::from_secs(settings.http_pool_idle_timeout_secs))
        .pool_max_idle_per_host(settings.http_pool_max_idle_per_host)
        .tcp_keepalive(Duration::from_secs(settings.http_tcp_keepalive_secs));

    if settings.http_enable_http2 {
        builder = builder.http2_adaptive_window(true);
    }

    builder.build().context("Failed to build HTTP client")
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
}

/// A text chunk produced by the chunker with its metadata.
#[derive(Debug, Clone)]
pub struct TextChunk {
    pub text: String,
    pub metadata: DocMetadata,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_result_serialization() {
        let result = SearchResult {
            text: "Hello world".to_string(),
            score: 0.95,
            metadata: SearchResultMetadata {
                file_path: "test.md".to_string(),
                category: "basics".to_string(),
                topic: "hello_world".to_string(),
                title: "Hello World".to_string(),
                has_code: true,
            },
        };
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["text"], "Hello world");
        assert_eq!(json["score"], 0.95);
        assert_eq!(json["metadata"]["category"], "basics");
        assert_eq!(json["metadata"]["has_code"], true);
    }

    #[test]
    fn test_index_metadata_deserialization() {
        let json =
            r#"{"version":"0.55.3","lang":"zh","embedding_model":"none","document_count":100}"#;
        let meta: IndexMetadata = serde_json::from_str(json).unwrap();
        assert_eq!(meta.version, "0.55.3");
        assert_eq!(meta.lang, "zh");
        assert_eq!(meta.document_count, 100);
        assert_eq!(meta.search_mode, "bm25"); // default
    }

    #[test]
    fn test_index_metadata_with_search_mode() {
        let json = r#"{"version":"dev","lang":"en","embedding_model":"openai:bge","document_count":50,"search_mode":"hybrid"}"#;
        let meta: IndexMetadata = serde_json::from_str(json).unwrap();
        assert_eq!(meta.search_mode, "hybrid");
    }

    #[test]
    fn test_doc_metadata_default() {
        let meta = DocMetadata::default();
        assert_eq!(meta.file_path, "");
        assert!(!meta.has_code);
    }
}
