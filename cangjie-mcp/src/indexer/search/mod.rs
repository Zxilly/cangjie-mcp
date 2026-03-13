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

use crate::api::client::HttpClient;
use crate::config::{DocLang, IndexInfo, Settings, DEFAULT_EMBEDDING_DIM};
use crate::indexer::embedding::{self, EmbedKind, Embedder};
use crate::indexer::rerank::{self, RerankerKind};
use crate::indexer::search::bm25::BM25Store;
use crate::indexer::search::fusion::reciprocal_rank_fusion;
use crate::indexer::search::vector::VectorStore;
use crate::indexer::SearchResult;
use crate::indexer::SearchResultMetadata;

/// Generate query variants using synonym expansion, returning up to `max_variants`
/// (including the original query).
fn generate_query_variants(query: &str, max_variants: usize) -> Vec<String> {
    use crate::indexer::search::synonyms::SYNONYM_MAP;

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
        let index_info = crate::indexer::initializer::initialize_and_index(&self.settings).await?;

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
            // Hybrid search: BM25 + Vector → RRF fusion (parallel)
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
    use crate::config::{DocLang, EmbeddingType, RerankType, Settings};
    use crate::indexer::TextChunk;
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
        use crate::indexer::DocMetadata;
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
                "仓颉编程语言的变量声明使用 let 关键字",
                "basics",
                "variables",
            ),
            make_chunk("函数定义使用 func 关键字来声明函数", "basics", "functions"),
            make_chunk("仓颉支持类和结构体的面向对象编程", "advanced", "classes"),
            make_chunk("泛型允许编写灵活可复用的代码", "advanced", "generics"),
            make_chunk("错误处理使用 try catch 机制", "basics", "error_handling"),
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

        let results = index.query("变量", 3, None, false).await.unwrap();
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
        let results = index.query("函数", 5, Some("basics"), false).await.unwrap();
        for r in &results {
            assert_eq!(
                r.metadata.category, "basics",
                "All results should belong to the 'basics' category"
            );
        }
    }

    #[tokio::test]
    async fn test_local_search_new_creates_instance() {
        let settings = test_settings(PathBuf::from("/tmp/test-search-new"));
        let index = LocalSearchIndex::new(settings).await;

        // A freshly created index has no stores loaded
        assert!(index.bm25_store.is_none());
        assert!(index.vector_store.is_none());
        // With EmbeddingType::None, no embedder is created
        assert!(index.embedder.is_none());
    }

    #[test]
    fn test_remote_search_new() {
        let remote = RemoteSearchIndex::new(
            &test_settings(PathBuf::from("/tmp")),
            "http://localhost:8765",
        )
        .unwrap();
        assert_eq!(remote.base_url(), "http://localhost:8765");
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

    #[test]
    fn test_remote_search_request_serialization() {
        let req = RemoteSearchRequest {
            query: "hello".to_string(),
            top_k: 5,
            rerank: true,
            category: Some("basics".to_string()),
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["query"], "hello");
        assert_eq!(json["top_k"], 5);
        assert_eq!(json["rerank"], true);
        assert_eq!(json["category"], "basics");
    }

    #[test]
    fn test_remote_search_request_no_category() {
        let req = RemoteSearchRequest {
            query: "hello".to_string(),
            top_k: 3,
            rerank: false,
            category: None,
        };
        let json = serde_json::to_value(&req).unwrap();
        assert!(
            json.get("category").is_none(),
            "None category should be skipped"
        );
    }

    #[test]
    fn test_remote_search_response_deserialization() {
        let json = r#"{"results":[{"text":"doc text","score":0.9,"metadata":{"file_path":"a.md","category":"cat","topic":"top","title":"Title","has_code":false}}]}"#;
        let resp: RemoteSearchResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.results.len(), 1);
        assert_eq!(resp.results[0].text, "doc text");
        assert!((resp.results[0].score - 0.9).abs() < f64::EPSILON);
        assert_eq!(resp.results[0].metadata.category, "cat");
    }

    #[test]
    fn test_remote_search_response_empty() {
        let json = r#"{"results":[]}"#;
        let resp: RemoteSearchResponse = serde_json::from_str(json).unwrap();
        assert!(resp.results.is_empty());
    }

    #[test]
    fn test_remote_info_response_deserialization() {
        let json = r#"{"version":"v0.55.4","lang":"zh","embedding_model":"none"}"#;
        let resp: RemoteInfoResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.version, "v0.55.4");
        assert_eq!(resp.lang, "zh");
        assert_eq!(resp.embedding_model, "none");
    }

    #[test]
    fn test_remote_info_response_defaults() {
        let json = r#"{}"#;
        let resp: RemoteInfoResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.version, "");
        assert_eq!(resp.lang, "");
        assert_eq!(resp.embedding_model, "");
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
        let results = index.query("编程", 2, None, false).await.unwrap();
        assert!(results.len() <= 2, "Should return at most top_k results");
    }

    #[tokio::test]
    async fn test_local_search_with_bm25_creates_instance() {
        let tmp = tempfile::tempdir().unwrap();
        let bm25 = BM25Store::new(tmp.path().join("bm25"));
        let settings = test_settings(PathBuf::from("/tmp/test-with-bm25"));

        let index = LocalSearchIndex::with_bm25(settings, bm25).await;
        assert!(
            index.bm25_store.is_some(),
            "BM25 store should be set via with_bm25"
        );
        assert!(index.vector_store.is_none());
        assert!(index.embedder.is_none());
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
            .query("变量", 5, Some("nonexistent_category"), false)
            .await
            .unwrap();
        assert!(
            results.is_empty(),
            "Query with nonexistent category should return empty results"
        );
    }
}
