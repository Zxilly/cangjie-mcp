pub mod bm25;
pub mod fusion;
mod sqlite_vec_ext;
pub mod synonyms;
pub mod vector;

use std::num::NonZeroUsize;
use std::sync::{Arc, LazyLock, Mutex as StdMutex};

use anyhow::{Context, Result};
use jieba_rs::Jieba;
use lru::LruCache;
use tracing::{info, warn};

/// Global Jieba instance shared across all search components.
pub static GLOBAL_JIEBA: LazyLock<Arc<Jieba>> = LazyLock::new(|| Arc::new(Jieba::new()));

const EMBEDDING_CACHE_SIZE: NonZeroUsize = NonZeroUsize::new(64).unwrap();

fn new_embedding_cache() -> StdMutex<LruCache<String, Vec<f32>>> {
    StdMutex::new(LruCache::new(EMBEDDING_CACHE_SIZE))
}

use crate::api_client::HttpClient;
use crate::embedding::{self, EmbedKind, Embedder};
use crate::rerank::{self, RerankerKind};
use crate::search::bm25::BM25Store;
use crate::search::fusion::reciprocal_rank_fusion;
use crate::search::vector::VectorStore;
use crate::SearchResult;
use crate::SearchResultMetadata;
use cangjie_core::config::{DocLang, IndexInfo, Settings, DEFAULT_EMBEDDING_DIM};

/// Generate query variants using synonym expansion, returning up to `max_variants`
/// (including the original query).
fn generate_query_variants(query: &str, max_variants: usize) -> Vec<String> {
    use crate::search::synonyms::SYNONYM_MAP;

    let lower = query.to_lowercase();
    let tokens: Vec<&str> = GLOBAL_JIEBA
        .cut_for_search(&lower, true)
        .into_iter()
        .filter(|w| !w.trim().is_empty())
        .collect();

    let mut variants = vec![query.to_string()];

    for (i, &token) in tokens.iter().enumerate() {
        let trimmed = token.trim();
        if let Some(group) = SYNONYM_MAP.get(trimmed) {
            for &synonym in group.iter().filter(|&&s| s != trimmed) {
                let mut new_tokens = tokens.clone();
                new_tokens[i] = synonym;
                let variant = new_tokens.join("");
                if !variants.contains(&variant) {
                    variants.push(variant);
                }
                if variants.len() >= max_variants {
                    return variants;
                }
            }
        }
        if variants.len() >= max_variants {
            break;
        }
    }

    variants
}

async fn bm25_multi_query_search(
    bm25: &BM25Store,
    query: &str,
    fetch_k: usize,
    category: Option<&str>,
    rrf_k: u32,
) -> Result<Vec<SearchResult>> {
    let variants = generate_query_variants(query, 3);
    let mut bm25_lists = Vec::with_capacity(variants.len());
    for variant in &variants {
        let results = bm25.search(variant, fetch_k, category).await?;
        bm25_lists.push(results);
    }
    Ok(reciprocal_rank_fusion(&bm25_lists, rrf_k, fetch_k))
}

// -- Local Search Index ------------------------------------------------------

pub struct LocalSearchIndex {
    settings: Settings,
    bm25_store: Option<BM25Store>,
    vector_store: Option<VectorStore>,
    embedder: Option<Box<dyn Embedder>>,
    reranker: RerankerKind,
    embedding_cache: StdMutex<LruCache<String, Vec<f32>>>,
}

impl LocalSearchIndex {
    /// Create a `LocalSearchIndex` with an injected BM25 store (for testing).
    #[doc(hidden)]
    pub async fn with_bm25(settings: Settings, bm25_store: BM25Store) -> Self {
        let reranker = rerank::create_reranker(&settings)
            .await
            .unwrap_or_else(|e| {
                warn!("Failed to create reranker: {}, using NoOp", e);
                RerankerKind::NoOp
            });
        Self {
            settings,
            bm25_store: Some(bm25_store),
            vector_store: None,
            embedder: None,
            reranker,
            embedding_cache: new_embedding_cache(),
        }
    }

    pub async fn new(settings: Settings) -> Self {
        let reranker = rerank::create_reranker(&settings)
            .await
            .unwrap_or_else(|e| {
                warn!("Failed to create reranker: {}, using NoOp", e);
                RerankerKind::NoOp
            });
        let embedder = embedding::create_embedder(&settings)
            .await
            .unwrap_or_else(|e| {
                warn!("Failed to create embedder: {}", e);
                None
            });
        Self {
            settings,
            bm25_store: None,
            vector_store: None,
            embedder,
            reranker,
            embedding_cache: new_embedding_cache(),
        }
    }

    pub async fn init(&mut self) -> Result<IndexInfo> {
        let index_info = crate::initializer::initialize_and_index(&self.settings).await?;

        let mut bm25 = BM25Store::new(index_info.bm25_index_dir());
        match bm25.load().await {
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
            let test = embedder.embed(&["test"], EmbedKind::Document).await?;
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

        let fetch_k = if rerank && self.reranker.is_enabled() {
            self.settings.rerank_initial_k.max(top_k)
        } else {
            top_k
        };

        let results = if has_bm25 && has_vector {
            // Hybrid search: BM25 + Vector -> RRF fusion (parallel)
            let bm25 = self
                .bm25_store
                .as_ref()
                .context("BM25 store not initialized")?;
            let embedder = self.embedder.as_ref().context("Embedder not initialized")?;
            let vector_store = self
                .vector_store
                .as_ref()
                .context("Vector store not initialized")?;

            let bm25_future =
                bm25_multi_query_search(bm25, query, fetch_k, category, self.settings.rrf_k);
            let vector_future = async {
                let query_emb = {
                    let cached = self.embedding_cache.lock().unwrap().get(query).cloned();
                    if let Some(emb) = cached {
                        emb
                    } else {
                        let emb = embedder.embed(&[query], EmbedKind::Query).await?;
                        let vec = emb.into_iter().next().context("Empty embedding result")?;
                        self.embedding_cache
                            .lock()
                            .unwrap()
                            .put(query.to_string(), vec.clone());
                        vec
                    }
                };
                vector_store.search(&query_emb, fetch_k, category).await
            };

            let (bm25_res, vector_res) = tokio::join!(bm25_future, vector_future);
            let bm25_results = bm25_res?;
            let vector_results = vector_res?;

            let mut fused = reciprocal_rank_fusion(
                &[bm25_results, vector_results],
                self.settings.rrf_k,
                fetch_k,
            );

            if rerank && self.reranker.is_enabled() && !fused.is_empty() {
                let fallback = fused.clone();
                fused = self
                    .reranker
                    .rerank(query, fused, top_k)
                    .await
                    .unwrap_or_else(|e| {
                        warn!("Reranking failed, returning fused results: {}", e);
                        fallback
                    });
            }

            fused
        } else if has_bm25 {
            // BM25 only (with multi-query synonym expansion)
            let bm25 = self
                .bm25_store
                .as_ref()
                .context("BM25 store not initialized")?;
            let results =
                bm25_multi_query_search(bm25, query, fetch_k, category, self.settings.rrf_k)
                    .await?;

            if rerank && self.reranker.is_enabled() && !results.is_empty() {
                match self.reranker.rerank(query, results.clone(), top_k).await {
                    Ok(reranked) => reranked,
                    Err(e) => {
                        warn!("Reranking failed, returning BM25 results: {}", e);
                        results
                    }
                }
            } else {
                results
            }
        } else {
            Vec::new()
        };

        // Sentence window expansion: fetch adjacent chunks for context
        let results = if let Some(ref vs) = self.vector_store {
            vector::expand_with_window(results, vs, 1).await
        } else {
            results
        };

        Ok(results)
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
    http: HttpClient,
}

impl RemoteSearchIndex {
    pub fn new(settings: &Settings, server_url: &str) -> Result<Self> {
        Ok(Self {
            http: HttpClient::new(settings, server_url, std::time::Duration::from_secs(60))?,
        })
    }

    pub fn base_url(&self) -> &str {
        self.http.base_url()
    }

    pub async fn init(&self) -> Result<IndexInfo> {
        info!("Connecting to remote server: {}", self.http.base_url());

        let data: RemoteInfoResponse = self.http.get_with_retry("info", 3).await?;
        let lang = match data.lang.as_str() {
            "en" => DocLang::En,
            _ => DocLang::Zh,
        };
        Ok(IndexInfo {
            version: data.version,
            lang,
            embedding_model_name: data.embedding_model,
            data_dir: cangjie_core::config::get_default_data_dir(),
        })
    }

    pub async fn query(
        &self,
        query: &str,
        top_k: usize,
        category: Option<&str>,
        rerank: bool,
    ) -> Result<Vec<SearchResult>> {
        let payload = RemoteSearchRequest {
            query: query.to_string(),
            top_k,
            rerank,
            category: category.map(|s| s.to_string()),
        };

        let resp = self.http.post("search").json(&payload).send().await?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TextChunk;
    use cangjie_core::config::{DocLang, EmbeddingType, RerankType, Settings};
    use std::path::PathBuf;

    fn test_settings(data_dir: PathBuf) -> Settings {
        Settings {
            data_dir,
            openai_base_url: "https://api.example.com".to_string(),
            openai_model: "test".to_string(),
            docs_lang: DocLang::Zh,
            embedding_type: EmbeddingType::None,
            rerank_type: RerankType::None,
            ..Settings::default()
        }
    }

    fn make_chunk(text: &str, category: &str, topic: &str) -> TextChunk {
        use crate::DocMetadata;
        let file_path = format!("{category}/{topic}.md");
        TextChunk {
            text: text.to_string(),
            metadata: DocMetadata {
                file_path: file_path.clone(),
                category: category.to_string(),
                topic: topic.to_string(),
                title: topic.to_string(),
                has_code: false,
                code_block_count: 0,
                chunk_id: format!("{file_path}#0"),
            },
        }
    }

    fn sample_chunks() -> Vec<TextChunk> {
        vec![
            make_chunk(
                "\u{4ed3}\u{9889}\u{7f16}\u{7a0b}\u{8bed}\u{8a00}\u{7684}\u{53d8}\u{91cf}\u{58f0}\u{660e}\u{4f7f}\u{7528} let \u{5173}\u{952e}\u{5b57}",
                "basics",
                "variables",
            ),
            make_chunk("\u{51fd}\u{6570}\u{5b9a}\u{4e49}\u{4f7f}\u{7528} func \u{5173}\u{952e}\u{5b57}\u{6765}\u{58f0}\u{660e}\u{51fd}\u{6570}", "basics", "functions"),
            make_chunk("\u{4ed3}\u{9889}\u{652f}\u{6301}\u{7c7b}\u{548c}\u{7ed3}\u{6784}\u{4f53}\u{7684}\u{9762}\u{5411}\u{5bf9}\u{8c61}\u{7f16}\u{7a0b}", "advanced", "classes"),
            make_chunk("\u{6cdb}\u{578b}\u{5141}\u{8bb8}\u{7f16}\u{5199}\u{7075}\u{6d3b}\u{53ef}\u{590d}\u{7528}\u{7684}\u{4ee3}\u{7801}", "advanced", "generics"),
            make_chunk("\u{9519}\u{8bef}\u{5904}\u{7406}\u{4f7f}\u{7528} try catch \u{673a}\u{5236}", "basics", "error_handling"),
        ]
    }

    async fn build_bm25_with_chunks(chunks: &[TextChunk]) -> BM25Store {
        let tmp = tempfile::tempdir().unwrap();
        let bm25_dir = tmp.path().join("bm25_index");
        let mut store = BM25Store::new(bm25_dir);
        store.build_from_chunks(chunks).await.unwrap();
        std::mem::forget(tmp); // keep temp dir alive
        store
    }

    #[tokio::test]
    async fn test_local_search_query_no_stores() {
        let settings = test_settings(PathBuf::from("/tmp/test-search"));
        let index = LocalSearchIndex {
            settings,
            bm25_store: None,
            vector_store: None,
            embedder: None,
            reranker: RerankerKind::NoOp,
            embedding_cache: new_embedding_cache(),
        };

        let results = index.query("test", 5, None, false).await.unwrap();
        assert!(
            results.is_empty(),
            "Expected empty results when no stores are configured"
        );
    }

    #[tokio::test]
    async fn test_local_search_query_bm25_only() {
        let chunks = sample_chunks();
        let bm25 = build_bm25_with_chunks(&chunks).await;
        let settings = test_settings(PathBuf::from("/tmp/test-search"));

        let index = LocalSearchIndex {
            settings,
            bm25_store: Some(bm25),
            vector_store: None,
            embedder: None,
            reranker: RerankerKind::NoOp,
            embedding_cache: new_embedding_cache(),
        };

        let results = index
            .query("\u{53d8}\u{91cf}", 3, None, false)
            .await
            .unwrap();
        assert!(
            !results.is_empty(),
            "BM25 search should return results for a matching query"
        );
    }

    #[tokio::test]
    async fn test_local_search_query_bm25_with_category() {
        let chunks = sample_chunks();
        let bm25 = build_bm25_with_chunks(&chunks).await;
        let settings = test_settings(PathBuf::from("/tmp/test-search"));

        let index = LocalSearchIndex {
            settings,
            bm25_store: Some(bm25),
            vector_store: None,
            embedder: None,
            reranker: RerankerKind::NoOp,
            embedding_cache: new_embedding_cache(),
        };

        // Search with category filter for "basics"
        let results = index
            .query("\u{51fd}\u{6570}", 5, Some("basics"), false)
            .await
            .unwrap();
        for r in &results {
            assert_eq!(
                r.metadata.category, "basics",
                "All results should belong to the 'basics' category"
            );
        }
    }

    #[test]
    fn test_remote_search_new_trailing_slash() {
        let remote = RemoteSearchIndex::new(
            &test_settings(PathBuf::from("/tmp")),
            "http://localhost:8765/",
        )
        .unwrap();
        assert_eq!(
            remote.base_url(),
            "http://localhost:8765",
            "Trailing slash should be trimmed"
        );
    }

    #[test]
    fn test_remote_search_new_multiple_trailing_slashes() {
        let remote = RemoteSearchIndex::new(
            &test_settings(PathBuf::from("/tmp")),
            "http://example.com///",
        )
        .unwrap();
        assert_eq!(
            remote.base_url(),
            "http://example.com",
            "All trailing slashes should be trimmed"
        );
    }

    #[tokio::test]
    async fn test_local_search_query_returns_top_k_results() {
        let chunks = sample_chunks();
        let bm25 = build_bm25_with_chunks(&chunks).await;
        let settings = test_settings(PathBuf::from("/tmp/test-search"));

        let index = LocalSearchIndex {
            settings,
            bm25_store: Some(bm25),
            vector_store: None,
            embedder: None,
            reranker: RerankerKind::NoOp,
            embedding_cache: new_embedding_cache(),
        };

        // Request top_k=2, should not return more than 2 results
        let results = index
            .query("\u{7f16}\u{7a0b}", 2, None, false)
            .await
            .unwrap();
        assert!(results.len() <= 2, "Should return at most top_k results");
    }

    #[tokio::test]
    async fn test_local_search_query_bm25_category_no_match() {
        // Query with a category that has no matching documents
        let chunks = sample_chunks();
        let bm25 = build_bm25_with_chunks(&chunks).await;
        let settings = test_settings(PathBuf::from("/tmp/test-search-cat-nomatch"));

        let index = LocalSearchIndex {
            settings,
            bm25_store: Some(bm25),
            vector_store: None,
            embedder: None,
            reranker: RerankerKind::NoOp,
            embedding_cache: new_embedding_cache(),
        };

        let results = index
            .query("\u{53d8}\u{91cf}", 5, Some("nonexistent_category"), false)
            .await
            .unwrap();
        assert!(
            results.is_empty(),
            "Query with nonexistent category should return empty results"
        );
    }
}
