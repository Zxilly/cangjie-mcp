use anyhow::Result;

use crate::indexer::SearchResult;

pub struct NoOpReranker;

impl NoOpReranker {
    pub fn rerank(&self, results: Vec<SearchResult>, top_k: usize) -> Result<Vec<SearchResult>> {
        Ok(results.into_iter().take(top_k).collect())
    }
}
