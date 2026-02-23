use std::sync::Arc;

use anyhow::{Context, Result};
use fastembed::{RerankerModel, TextRerank};
use tracing::info;

use crate::indexer::SearchResult;

pub struct LocalReranker {
    model: Arc<TextRerank>,
}

impl LocalReranker {
    pub async fn new() -> Result<Self> {
        info!("Loading local reranker model: BAAI/bge-reranker-v2-m3");
        let model = tokio::task::spawn_blocking(|| {
            TextRerank::try_new(
                fastembed::RerankInitOptions::new(RerankerModel::BGERerankerV2M3)
                    .with_show_download_progress(true),
            )
            .context("Failed to load local reranker model")
        })
        .await
        .context("Reranker loading task panicked")??;

        Ok(Self {
            model: Arc::new(model),
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

        info!("Reranking {} results with local model...", results.len());

        let model = Arc::clone(&self.model);
        let query = query.to_string();

        tokio::task::spawn_blocking(move || {
            let documents: Vec<&str> = results.iter().map(|r| r.text.as_str()).collect();
            let reranked = model
                .rerank(&query, documents, true, None)
                .context("Local reranking failed")?;

            let mut output: Vec<SearchResult> = reranked
                .into_iter()
                .filter_map(|r| {
                    let idx = r.index;
                    if idx < results.len() {
                        let mut result = results[idx].clone();
                        result.score = r.score as f64;
                        Some(result)
                    } else {
                        None
                    }
                })
                .take(top_k)
                .collect();

            output.sort_by(|a, b| {
                b.score
                    .partial_cmp(&a.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            info!("Local reranking complete.");
            Ok(output)
        })
        .await
        .context("Reranking task panicked")?
    }
}
