use anyhow::{Context, Result};
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use tracing::info;

use super::Embedder;

pub struct LocalEmbedder {
    model: TextEmbedding,
    model_name: String,
}

impl LocalEmbedder {
    pub fn new(model_name: &str) -> Result<Self> {
        let model_enum = match model_name {
            "BAAI/bge-small-zh-v1.5" | "bge-small-zh-v1.5" => EmbeddingModel::BGESmallZHV15,
            "intfloat/multilingual-e5-small" | "multilingual-e5-small" => {
                EmbeddingModel::MultilingualE5Small
            }
            "intfloat/multilingual-e5-base" | "multilingual-e5-base" => {
                EmbeddingModel::MultilingualE5Base
            }
            "intfloat/multilingual-e5-large" | "multilingual-e5-large" => {
                EmbeddingModel::MultilingualE5Large
            }
            _ => EmbeddingModel::ParaphraseMLMiniLML12V2,
        };

        info!("Loading local embedding model: {}", model_name);
        let model =
            TextEmbedding::try_new(InitOptions::new(model_enum).with_show_download_progress(true))
                .context("Failed to load local embedding model")?;

        Ok(Self {
            model,
            model_name: model_name.to_string(),
        })
    }
}

impl Embedder for LocalEmbedder {
    fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let docs: Vec<String> = texts.iter().map(|t| t.to_string()).collect();
        self.model
            .embed(docs, None)
            .context("Local embedding failed")
    }

    fn model_name(&self) -> &str {
        &self.model_name
    }
}
