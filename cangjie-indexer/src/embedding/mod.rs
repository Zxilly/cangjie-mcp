pub mod openai;

use anyhow::Result;
use async_trait::async_trait;

use cangjie_core::config::{EmbeddingType, Settings};

// -- Embedder trait ----------------------------------------------------------

/// Distinguish query embedding from document embedding for asymmetric retrieval models.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbedKind {
    /// User query
    Query,
    /// Indexed document content
    Document,
}

/// Async embedding trait.
#[async_trait]
pub trait Embedder: Send + Sync {
    async fn embed(&self, texts: &[&str], kind: EmbedKind) -> Result<Vec<Vec<f32>>>;
    fn model_name(&self) -> &str;

    /// Conservative character-level input limit for the model.
    /// Returns `None` for unknown models (falls back to user config).
    fn max_input_chars(&self) -> Option<usize> {
        None
    }
}

/// Conservative character-level input limit for known embedding models.
///
/// Uses a conservative 1.5 chars/token ratio to ensure chunks never exceed
/// the model's token limit. Returns `None` for unknown models.
pub fn model_max_input_chars(model_name: &str) -> Option<usize> {
    match model_name {
        s if s.contains("MiniLM") => Some(180),              // 128 tokens
        s if s.contains("bge-m3") => Some(12000),            // 8192 tokens
        s if s.contains("bge-large") => Some(750),           // 512 tokens
        s if s.contains("bge-small") => Some(750),           // 512 tokens
        s if s.contains("e5") => Some(750),                  // 512 tokens
        s if s.contains("text-embedding-3") => Some(12000),  // 8191 tokens
        s if s.contains("text-embedding-ada") => Some(12000), // 8191 tokens
        _ => None,
    }
}

// -- Factory -----------------------------------------------------------------

/// Create an embedder based on settings.
///
/// Returns `None` if embedding is disabled (`EmbeddingType::None`).
pub async fn create_embedder(settings: &Settings) -> Result<Option<Box<dyn Embedder>>> {
    match settings.embedding_type {
        EmbeddingType::None => Ok(None),
        EmbeddingType::OpenAI => {
            let api_key = settings
                .openai_api_key
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("OpenAI API key required for openai embedding"))?;
            Ok(Some(Box::new(openai::OpenAIEmbedder::new(
                settings,
                api_key,
                &settings.openai_model,
                &settings.openai_base_url,
            )?)))
        }
        EmbeddingType::Local => {
            #[cfg(feature = "local")]
            {
                let embedder = local::LocalEmbedder::new(&settings.local_model).await?;
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
            embedding_type,
            local_model: "test-model".to_string(),
            data_dir: std::path::PathBuf::from("/tmp"),
            openai_base_url: "https://api.example.com".to_string(),
            openai_model: "test".to_string(),
            ..Settings::default()
        }
    }

    #[tokio::test]
    async fn test_create_embedder_none() {
        let settings = test_settings(EmbeddingType::None);
        let embedder = create_embedder(&settings).await.unwrap();
        assert!(embedder.is_none());
    }

    #[tokio::test]
    async fn test_create_embedder_openai_no_key() {
        let settings = test_settings(EmbeddingType::OpenAI);
        let result = create_embedder(&settings).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_model_max_input_chars_bge_m3() {
        assert_eq!(model_max_input_chars("BAAI/bge-m3"), Some(12000));
    }

    #[test]
    fn test_model_max_input_chars_bge_large() {
        assert_eq!(model_max_input_chars("BAAI/bge-large-zh-v1.5"), Some(750));
    }

    #[test]
    fn test_model_max_input_chars_minilm() {
        assert_eq!(
            model_max_input_chars("paraphrase-multilingual-MiniLM-L12-v2"),
            Some(180)
        );
    }

    #[test]
    fn test_model_max_input_chars_text_embedding_3() {
        assert_eq!(model_max_input_chars("text-embedding-3-small"), Some(12000));
    }

    #[test]
    fn test_model_max_input_chars_e5() {
        assert_eq!(model_max_input_chars("multilingual-e5-large"), Some(750));
    }

    #[test]
    fn test_model_max_input_chars_unknown() {
        assert_eq!(model_max_input_chars("unknown-model-xyz"), None);
    }
}
