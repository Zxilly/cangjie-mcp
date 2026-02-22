use anyhow::Result;
use serde::Deserialize;
use tracing::info;

use crate::indexer::SearchResult;

pub struct OpenAIReranker {
    api_key: String,
    model: String,
    base_url: String,
}

impl OpenAIReranker {
    pub fn new(api_key: &str, model: &str, base_url: &str) -> Self {
        Self {
            api_key: api_key.to_string(),
            model: model.to_string(),
            base_url: base_url.trim_end_matches('/').to_string(),
        }
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
            self.base_url
        );

        let documents: Vec<&str> = results.iter().map(|r| r.text.as_str()).collect();
        let url = format!("{}/rerank", self.base_url);

        let client = reqwest::Client::new();
        let response = client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "model": self.model,
                "query": query,
                "documents": documents,
                "top_n": top_k,
                "return_documents": false,
            }))
            .timeout(std::time::Duration::from_secs(30))
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

#[derive(Debug, Deserialize)]
struct RerankResponse {
    results: Vec<RerankItem>,
}

#[derive(Debug, Deserialize)]
struct RerankItem {
    index: usize,
    relevance_score: f64,
}
