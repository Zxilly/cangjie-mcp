use anyhow::Result;
use async_trait::async_trait;
use tracing::info;

use super::{EmbedKind, Embedder};
use crate::config::Settings;
use crate::indexer::api_client::ApiClient;
use crate::indexer::api_types::EmbeddingsResponse;
use crate::indexer::build_http_client;

pub struct OpenAIEmbedder {
    api: ApiClient,
}

impl OpenAIEmbedder {
    pub fn new(settings: &Settings, api_key: &str, model: &str, base_url: &str) -> Result<Self> {
        let client = build_http_client(settings, std::time::Duration::from_secs(120))?;
        Ok(Self {
            api: ApiClient::new(client, api_key, model, base_url),
        })
    }
}

#[async_trait]
impl Embedder for OpenAIEmbedder {
    async fn embed(&self, texts: &[&str], kind: EmbedKind) -> Result<Vec<Vec<f32>>> {
        info!(
            "Getting embeddings for {} texts ({:?}) via {}",
            texts.len(),
            kind,
            self.api.base_url()
        );

        let mut payload = serde_json::json!({
            "model": self.api.model(),
            "input": texts,
        });
        match kind {
            EmbedKind::Query => {
                payload["input_type"] = serde_json::json!("query");
            }
            EmbedKind::Document => {
                payload["input_type"] = serde_json::json!("passage");
            }
        }

        let response = self.api.post("embeddings").json(&payload).send().await?;

        let body: EmbeddingsResponse = response.json().await?;
        Ok(body.data.into_iter().map(|d| d.embedding).collect())
    }

    fn model_name(&self) -> &str {
        self.api.model()
    }
}
