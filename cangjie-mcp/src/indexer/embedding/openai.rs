use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use tracing::info;

use super::Embedder;
use crate::config::Settings;
use crate::indexer::build_http_client;

pub struct OpenAIEmbedder {
    api_key: String,
    model: String,
    base_url: String,
    client: reqwest::Client,
}

impl OpenAIEmbedder {
    pub fn new(settings: &Settings, api_key: &str, model: &str, base_url: &str) -> Result<Self> {
        Ok(Self {
            api_key: api_key.to_string(),
            model: model.to_string(),
            base_url: base_url.trim_end_matches('/').to_string(),
            client: build_http_client(settings, std::time::Duration::from_secs(120))?,
        })
    }
}

#[async_trait]
impl Embedder for OpenAIEmbedder {
    async fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let url = format!("{}/embeddings", self.base_url);
        info!(
            "Getting embeddings for {} texts via {}",
            texts.len(),
            self.base_url
        );

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "model": self.model,
                "input": texts,
            }))
            .send()
            .await?;

        let body: EmbeddingsResponse = response.json().await?;
        Ok(body.data.into_iter().map(|d| d.embedding).collect())
    }

    fn model_name(&self) -> &str {
        &self.model
    }
}

#[derive(Debug, Deserialize)]
struct EmbeddingsResponse {
    data: Vec<EmbeddingData>,
}

#[derive(Debug, Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
}
