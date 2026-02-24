pub mod noop;
pub mod openai;

use anyhow::Result;

use crate::config::{RerankType, Settings};
use crate::indexer::SearchResult;

// -- RerankerKind enum -------------------------------------------------------

/// Enum dispatch for reranking — the variant set is known and fixed,
/// which avoids async trait object-safety issues.
pub enum RerankerKind {
    NoOp,
    OpenAI(openai::OpenAIReranker),
    #[cfg(feature = "local")]
    Local(Box<local::LocalReranker>),
}

impl RerankerKind {
    pub async fn rerank(
        &self,
        query: &str,
        results: Vec<SearchResult>,
        top_k: usize,
    ) -> Result<Vec<SearchResult>> {
        match self {
            RerankerKind::NoOp => noop::NoOpReranker.rerank(results, top_k),
            RerankerKind::OpenAI(r) => r.rerank(query, results, top_k).await,
            #[cfg(feature = "local")]
            RerankerKind::Local(r) => r.rerank(query, results, top_k).await,
        }
    }

    pub fn is_enabled(&self) -> bool {
        !matches!(self, RerankerKind::NoOp)
    }
}

// -- Factory -----------------------------------------------------------------

pub async fn create_reranker(settings: &Settings) -> Result<RerankerKind> {
    match settings.rerank_type {
        RerankType::None => Ok(RerankerKind::NoOp),
        RerankType::OpenAI => {
            let api_key = settings
                .openai_api_key
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("OpenAI API key required for openai reranking"))?;
            Ok(RerankerKind::OpenAI(openai::OpenAIReranker::new(
                settings,
                api_key,
                &settings.rerank_model,
                &settings.openai_base_url,
            )?))
        }
        RerankType::Local => {
            #[cfg(feature = "local")]
            {
                let reranker = local::LocalReranker::new().await?;
                Ok(RerankerKind::Local(Box::new(reranker)))
            }
            #[cfg(not(feature = "local"))]
            {
                tracing::warn!("Local reranking requires 'local' feature, using NoOp");
                Ok(RerankerKind::NoOp)
            }
        }
    }
}

#[cfg(feature = "local")]
pub mod local;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{DocLang, EmbeddingType};
    use crate::indexer::{SearchResult, SearchResultMetadata};

    fn test_settings(rerank_type: RerankType) -> Settings {
        Settings {
            rerank_type,
            rerank_model: "test-model".to_string(),
            data_dir: std::path::PathBuf::from("/tmp"),
            openai_base_url: "https://api.example.com".to_string(),
            openai_model: "test".to_string(),
            docs_lang: DocLang::Zh,
            embedding_type: EmbeddingType::None,
            ..Settings::default()
        }
    }

    fn make_result(text: &str, score: f64) -> SearchResult {
        SearchResult {
            text: text.to_string(),
            score,
            metadata: SearchResultMetadata {
                file_path: "test.md".to_string(),
                category: "test".to_string(),
                topic: "test".to_string(),
                title: "Test".to_string(),
                has_code: false,
            },
        }
    }

    #[tokio::test]
    async fn test_create_noop_reranker() {
        let settings = test_settings(RerankType::None);
        let reranker = create_reranker(&settings).await.unwrap();
        assert!(!reranker.is_enabled());
    }

    #[tokio::test]
    async fn test_create_openai_reranker_no_key() {
        let settings = test_settings(RerankType::OpenAI);
        let result = create_reranker(&settings).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_noop_reranker_passthrough() {
        let reranker = RerankerKind::NoOp;
        let results = vec![
            make_result("doc1", 0.9),
            make_result("doc2", 0.8),
            make_result("doc3", 0.7),
        ];
        let reranked = reranker.rerank("query", results, 2).await.unwrap();
        assert_eq!(reranked.len(), 2);
        assert_eq!(reranked[0].text, "doc1");
        assert_eq!(reranked[1].text, "doc2");
    }
}
