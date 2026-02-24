use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use async_trait::async_trait;
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
#[cfg(feature = "legacy")]
use once_cell::sync::OnceCell;
use tracing::info;

use super::Embedder;

pub struct LocalEmbedder {
    model: Arc<Mutex<TextEmbedding>>,
    model_name: String,
}

impl LocalEmbedder {
    pub async fn new(model_name: &str) -> Result<Self> {
        init_ort_backend();

        let model_enum: EmbeddingModel = model_name.parse().map_err(|e: String| {
            anyhow::anyhow!("Unsupported embedding model '{}': {}", model_name, e)
        })?;

        info!("Loading local embedding model: {}", model_name);
        let name = model_name.to_string();
        let model = tokio::task::spawn_blocking(move || {
            TextEmbedding::try_new(InitOptions::new(model_enum).with_show_download_progress(true))
                .context("Failed to load local embedding model")
        })
        .await
        .context("Model loading task panicked")??;

        Ok(Self {
            model: Arc::new(Mutex::new(model)),
            model_name: name,
        })
    }
}

#[cfg(feature = "legacy")]
pub(crate) fn init_ort_backend() {
    static INIT: OnceCell<()> = OnceCell::new();
    INIT.get_or_init(|| {
        ort::set_api(ort_tract::api());
        info!("Using legacy inference backend: ort-tract");
    });
}

#[cfg(not(feature = "legacy"))]
pub(crate) fn init_ort_backend() {}

#[async_trait]
impl Embedder for LocalEmbedder {
    async fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let docs: Vec<String> = texts.iter().map(|t| t.to_string()).collect();
        let model = Arc::clone(&self.model);
        tokio::task::spawn_blocking(move || {
            let mut model = model
                .lock()
                .map_err(|e| anyhow::anyhow!("Embedding model lock poisoned: {}", e))?;
            model.embed(docs, None).context("Local embedding failed")
        })
        .await
        .context("Embedding task panicked")?
    }

    fn model_name(&self) -> &str {
        &self.model_name
    }
}
