pub mod openai;

use anyhow::Result;

use crate::config::{EmbeddingType, Settings};

// -- Embedder trait ----------------------------------------------------------

/// Synchronous embedding trait.
///
/// fastembed is CPU synchronous; OpenAI uses blocking Client.
pub trait Embedder: Send + Sync {
    fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;
    fn model_name(&self) -> &str;
}

// -- Factory -----------------------------------------------------------------

/// Create an embedder based on settings.
///
/// Returns `None` if embedding is disabled (`EmbeddingType::None`).
pub fn create_embedder(settings: &Settings) -> Result<Option<Box<dyn Embedder>>> {
    match settings.embedding_type {
        EmbeddingType::None => Ok(None),
        EmbeddingType::OpenAI => {
            let api_key = settings
                .openai_api_key
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("OpenAI API key required for openai embedding"))?;
            Ok(Some(Box::new(openai::OpenAIEmbedder::new(
                api_key,
                &settings.openai_model,
                &settings.openai_base_url,
            )?)))
        }
        EmbeddingType::Local => {
            #[cfg(feature = "local")]
            {
                let embedder = local::LocalEmbedder::new(&settings.local_model)?;
                Ok(Some(Box::new(embedder)))
            }
            #[cfg(not(feature = "local"))]
            {
                anyhow::bail!("Local embedding requires the 'local' feature")
            }
        }
    }
}

#[cfg(feature = "local")]
pub mod local;

#[cfg(test)]
mod tests {
    use super::*;

    fn test_settings(embedding_type: EmbeddingType) -> Settings {
        Settings {
            docs_version: "dev".to_string(),
            docs_lang: crate::config::DocLang::Zh,
            embedding_type,
            local_model: "test-model".to_string(),
            rerank_type: crate::config::RerankType::None,
            rerank_model: "".to_string(),
            rerank_top_k: 5,
            rerank_initial_k: 20,
            rrf_k: 60,
            chunk_max_size: 6000,
            data_dir: std::path::PathBuf::from("/tmp"),
            server_url: None,
            openai_api_key: None,
            openai_base_url: "https://api.example.com".to_string(),
            openai_model: "test".to_string(),
        }
    }

    #[test]
    fn test_create_embedder_none() {
        let settings = test_settings(EmbeddingType::None);
        let embedder = create_embedder(&settings).unwrap();
        assert!(embedder.is_none());
    }

    #[test]
    fn test_create_embedder_openai_no_key() {
        let settings = test_settings(EmbeddingType::OpenAI);
        let result = create_embedder(&settings);
        assert!(result.is_err());
    }
}
