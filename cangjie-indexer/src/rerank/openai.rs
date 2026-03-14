use anyhow::Result;
use tracing::info;

use crate::api_client::ApiClient;
use crate::SearchResult;
use cangjie_core::api_types::RerankResponse;
use cangjie_core::config::Settings;

pub struct OpenAIReranker {
    api: ApiClient,
}

impl OpenAIReranker {
    pub fn new(settings: &Settings, api_key: &str, model: &str, base_url: &str) -> Result<Self> {
        Ok(Self {
            api: ApiClient::new(
                settings,
                api_key,
                model,
                base_url,
                std::time::Duration::from_secs(30),
            )?,
        })
    }

    pub async fn rerank(
        &self,
        query: &str,
        results: Vec<SearchResult>,
        top_k: usize,
    ) -> Result<Vec<SearchResult>> {
        if results.is_empty() {
            return Ok(Vec::new());
        }

        info!(
            "Reranking {} results with API ({})...",
            results.len(),
            self.api.base_url()
        );

        let documents: Vec<&str> = results
            .iter()
            .map(|r| crate::document::chunker::strip_chunk_artifacts(&r.text))
            .collect();

        let response = self
            .api
            .post("rerank")
            .json(&serde_json::json!({
                "model": self.api.model(),
                "query": query,
                "documents": documents,
                "top_n": top_k,
                "return_documents": false,
            }))
            .send()
            .await?;

        let body: RerankResponse = response.json().await?;

        let mut reranked = Vec::new();
        for item in body.results {
            if item.index < results.len() {
                let mut result = results[item.index].clone();
                result.score = item.relevance_score;
                reranked.push(result);
            }
        }

        info!("Reranking complete.");
        Ok(reranked)
    }
}
