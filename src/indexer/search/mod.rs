pub mod bm25;
pub mod fusion;
pub mod vector;

use anyhow::{Context, Result};
use tracing::{info, warn};

use crate::config::{DocLang, IndexInfo, Settings, DEFAULT_EMBEDDING_DIM};
use crate::indexer::embedding::{self, Embedder};
use crate::indexer::rerank::{self, RerankerKind};
use crate::indexer::search::bm25::BM25Store;
use crate::indexer::search::fusion::reciprocal_rank_fusion;
use crate::indexer::search::vector::VectorStore;
use crate::indexer::SearchResult;
use crate::indexer::SearchResultMetadata;

// -- Local Search Index ------------------------------------------------------

pub struct LocalSearchIndex {
    settings: Settings,
    bm25_store: Option<BM25Store>,
    vector_store: Option<VectorStore>,
    embedder: Option<Box<dyn Embedder>>,
    reranker: RerankerKind,
}

impl LocalSearchIndex {
    pub fn new(settings: Settings) -> Self {
        let reranker = rerank::create_reranker(&settings).unwrap_or_else(|e| {
            warn!("Failed to create reranker: {}, using NoOp", e);
            RerankerKind::NoOp
        });
        let embedder = embedding::create_embedder(&settings).unwrap_or_else(|e| {
            warn!("Failed to create embedder: {}", e);
            None
        });
        Self {
            settings,
            bm25_store: None,
            vector_store: None,
            embedder,
            reranker,
        }
    }

    pub fn init(&mut self) -> Result<IndexInfo> {
        let index_info = crate::indexer::initializer::initialize_and_index(&self.settings)?;

        // Load BM25 store
        let mut bm25 = BM25Store::new(index_info.bm25_index_dir());
        match bm25.load() {
            Ok(true) => {
                self.bm25_store = Some(bm25);
            }
            Ok(false) => {
                warn!("BM25 index not found at {:?}", index_info.bm25_index_dir());
            }
            Err(e) => {
                warn!("Failed to load BM25 index: {}", e);
            }
        }

        Ok(index_info)
    }

    /// Async initialization for vector store (call after init).
    pub async fn init_vector_store(&mut self, index_info: &IndexInfo) -> Result<()> {
        if self.embedder.is_none() {
            return Ok(());
        }

        let vector_dir = index_info.vector_db_dir();
        // Determine embedding dimension by doing a test embed
        let dim = if let Some(ref embedder) = self.embedder {
            let test = embedder.embed(&["test"])?;
            test.first()
                .map(|v| v.len())
                .unwrap_or(DEFAULT_EMBEDDING_DIM)
        } else {
            DEFAULT_EMBEDDING_DIM
        };

        let vs = VectorStore::open(&vector_dir, dim).await?;
        if vs.is_ready() {
            info!("Vector store loaded from {:?}", vector_dir);
            self.vector_store = Some(vs);
        } else {
            info!("Vector store not found, will be built during indexing");
        }

        Ok(())
    }

    pub async fn query(
        &self,
        query: &str,
        top_k: usize,
        category: Option<&str>,
        rerank: bool,
    ) -> Result<Vec<SearchResult>> {
        let has_bm25 = self.bm25_store.is_some();
        let has_vector = self.vector_store.is_some() && self.embedder.is_some();

        if !has_bm25 && !has_vector {
            return Ok(Vec::new());
        }

        // Hybrid search: BM25 + Vector → RRF fusion
        if has_bm25 && has_vector {
            let bm25 = self
                .bm25_store
                .as_ref()
                .context("BM25 store not initialized")?;
            let bm25_results = bm25.search(query, top_k, category)?;

            let embedder = self.embedder.as_ref().context("Embedder not initialized")?;
            let query_emb = embedder.embed(&[query])?;
            let vector_store = self
                .vector_store
                .as_ref()
                .context("Vector store not initialized")?;
            let vector_results = vector_store.search(&query_emb[0], top_k, category).await?;

            let mut fused =
                reciprocal_rank_fusion(&[bm25_results, vector_results], self.settings.rrf_k, top_k);

            if rerank && self.reranker.is_enabled() && !fused.is_empty() {
                fused = self
                    .reranker
                    .rerank(query, fused, top_k)
                    .await
                    .unwrap_or_else(|e| {
                        warn!("Reranking failed: {}", e);
                        Vec::new()
                    });
            }

            return Ok(fused);
        }

        // BM25 only
        if has_bm25 {
            let bm25 = self
                .bm25_store
                .as_ref()
                .context("BM25 store not initialized")?;
            let results = bm25.search(query, top_k, category)?;

            if rerank && self.reranker.is_enabled() && !results.is_empty() {
                return self
                    .reranker
                    .rerank(query, results, top_k)
                    .await
                    .or_else(|e| {
                        warn!("Reranking failed, returning BM25 results: {}", e);
                        bm25.search(query, top_k, category)
                    });
            }

            return Ok(results);
        }

        Ok(Vec::new())
    }
}

// -- Remote protocol types ---------------------------------------------------

#[derive(Debug, serde::Deserialize)]
struct RemoteInfoResponse {
    #[serde(default)]
    version: String,
    #[serde(default)]
    lang: String,
    #[serde(default)]
    embedding_model: String,
}

#[derive(Debug, serde::Serialize)]
struct RemoteSearchRequest {
    query: String,
    top_k: usize,
    rerank: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    category: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct RemoteSearchResponse {
    #[serde(default)]
    results: Vec<RemoteSearchResultItem>,
}

#[derive(Debug, serde::Deserialize)]
struct RemoteSearchResultItem {
    #[serde(default)]
    text: String,
    #[serde(default)]
    score: f64,
    #[serde(default)]
    metadata: SearchResultMetadata,
}

// -- Remote Search Index -----------------------------------------------------

pub struct RemoteSearchIndex {
    server_url: String,
    client: reqwest::Client,
}

impl RemoteSearchIndex {
    pub fn new(server_url: &str) -> Result<Self> {
        Ok(Self {
            server_url: server_url.trim_end_matches('/').to_string(),
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(60))
                .build()
                .context("Failed to build HTTP client")?,
        })
    }

    pub fn init(&self) -> Result<IndexInfo> {
        let url = format!("{}/info", self.server_url);
        info!("Connecting to remote server: {}", self.server_url);
        let resp = reqwest::blocking::Client::new()
            .get(&url)
            .timeout(std::time::Duration::from_secs(30))
            .send()
            .context("Failed to connect to remote server")?;
        let data: RemoteInfoResponse = resp.json().context("Invalid /info response")?;

        let lang = match data.lang.as_str() {
            "en" => DocLang::En,
            _ => DocLang::Zh,
        };

        Ok(IndexInfo {
            version: data.version,
            lang,
            embedding_model_name: data.embedding_model,
            data_dir: crate::config::get_default_data_dir(),
        })
    }

    pub async fn query(
        &self,
        query: &str,
        top_k: usize,
        category: Option<&str>,
        rerank: bool,
    ) -> Result<Vec<SearchResult>> {
        let url = format!("{}/search", self.server_url);
        let payload = RemoteSearchRequest {
            query: query.to_string(),
            top_k,
            rerank,
            category: category.map(|s| s.to_string()),
        };

        let resp = self.client.post(&url).json(&payload).send().await?;
        let data: RemoteSearchResponse = resp.json().await.context("Invalid /search response")?;

        Ok(data
            .results
            .into_iter()
            .map(|item| SearchResult {
                text: item.text,
                score: item.score,
                metadata: item.metadata,
            })
            .collect())
    }
}
