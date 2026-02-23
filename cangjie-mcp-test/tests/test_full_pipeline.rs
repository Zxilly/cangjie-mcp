//! End-to-end integration tests for the full indexing and search pipeline.
//!
//! These tests clone the real cangjie_docs repository, build a BM25 index,
//! and verify search works correctly against real documentation.
//!
//! Run with: `cargo test -p cangjie-mcp-test --test test_full_pipeline -- --ignored`

use axum::body::Body;
use axum::http::{Request, StatusCode};
use cangjie_mcp::config::{DocLang, EmbeddingType, RerankType, Settings};
use cangjie_mcp::indexer::document::chunker::chunk_documents;
use cangjie_mcp::indexer::document::source::{DocumentSource, GitDocumentSource};
use cangjie_mcp::indexer::search::bm25::BM25Store;
use cangjie_mcp::indexer::search::LocalSearchIndex;
use cangjie_mcp::indexer::IndexMetadata;
use cangjie_mcp::repo::GitManager;
use cangjie_mcp::server::http::create_http_app;
use http_body_util::BodyExt;
use tempfile::TempDir;
use tower::ServiceExt;

fn real_settings(data_dir: std::path::PathBuf) -> Settings {
    Settings {
        docs_version: "latest".to_string(),
        docs_lang: DocLang::Zh,
        embedding_type: EmbeddingType::None,
        local_model: String::new(),
        rerank_type: RerankType::None,
        rerank_model: String::new(),
        rerank_top_k: 5,
        rerank_initial_k: 20,
        rrf_k: 60,
        chunk_max_size: 6000,
        data_dir,
        server_url: None,
        openai_api_key: None,
        openai_base_url: "https://api.siliconflow.cn/v1".to_string(),
        openai_model: String::new(),
        prebuilt: cangjie_mcp::config::PrebuiltMode::Off,
    }
}

/// Clone repo, load docs, chunk, build BM25 index. Returns everything needed for search.
fn build_real_index() -> (TempDir, BM25Store, Box<dyn DocumentSource>) {
    let tmp = TempDir::new().unwrap();

    // Clone and checkout
    let repo_dir = tmp.path().join("docs_repo");
    let mut git_mgr = GitManager::new(repo_dir.clone());
    git_mgr.ensure_cloned(false).unwrap();

    // Load documents
    let source = GitDocumentSource::new(repo_dir, DocLang::Zh).unwrap();
    let docs = source.load_all_documents().unwrap();
    assert!(docs.len() > 10, "should load many documents");

    // Chunk
    let chunks = chunk_documents(&docs, 6000);
    assert!(
        chunks.len() > docs.len(),
        "chunking should produce more chunks than docs"
    );

    // Build BM25
    let bm25_dir = tmp.path().join("bm25_index");
    let mut bm25 = BM25Store::new(bm25_dir);
    bm25.build_from_chunks(&chunks).unwrap();

    // Rebuild source for trait object (can't move out of `source` after loading docs)
    let source2 = GitDocumentSource::new(tmp.path().join("docs_repo"), DocLang::Zh).unwrap();
    (tmp, bm25, Box::new(source2))
}

#[test]
#[ignore]
fn test_real_docs_bm25_search_chinese() {
    let (_tmp, store, _source) = build_real_index();

    let results = store.search("函数定义", 5, None).unwrap();
    assert!(
        !results.is_empty(),
        "searching '函数定义' in real docs should return results"
    );
    assert!(results[0].score > 0.0);
}

#[test]
#[ignore]
fn test_real_docs_bm25_search_english_keyword() {
    let (_tmp, store, _source) = build_real_index();

    // English keywords like "Array", "HashMap" should appear in Chinese docs
    let results = store.search("Array", 5, None).unwrap();
    assert!(
        !results.is_empty(),
        "searching 'Array' in real docs should return results"
    );
}

#[test]
#[ignore]
fn test_real_docs_bm25_search_with_category() {
    let (_tmp, store, _source) = build_real_index();

    // Get a result first to find a real category
    let all_results = store.search("仓颉", 10, None).unwrap();
    assert!(!all_results.is_empty());

    let cat = &all_results[0].metadata.category;
    let filtered = store.search("仓颉", 10, Some(cat)).unwrap();
    for r in &filtered {
        assert_eq!(
            &r.metadata.category, cat,
            "category filter should be applied"
        );
    }
}

#[test]
#[ignore]
fn test_real_docs_search_relevance() {
    let (_tmp, store, _source) = build_real_index();

    let results = store.search("错误处理 异常", 5, None).unwrap();
    assert!(!results.is_empty());

    // At least one result should contain error/exception related content
    let relevant = results.iter().any(|r| {
        r.text.contains("错误")
            || r.text.contains("异常")
            || r.text.contains("Error")
            || r.text.contains("Exception")
            || r.text.contains("try")
    });
    assert!(
        relevant,
        "search for error handling should return relevant results"
    );
}

#[tokio::test]
#[ignore]
async fn test_real_docs_local_search_index_query() {
    let tmp = TempDir::new().unwrap();
    let settings = real_settings(tmp.path().to_path_buf());

    let repo_dir = tmp.path().join("docs_repo");
    let mut git_mgr = GitManager::new(repo_dir.clone());
    git_mgr.ensure_cloned(false).unwrap();

    let source = GitDocumentSource::new(repo_dir, DocLang::Zh).unwrap();
    let docs = source.load_all_documents().unwrap();
    let chunks = chunk_documents(&docs, 6000);

    let bm25_dir = tmp.path().join("bm25_index");
    let mut bm25 = BM25Store::new(bm25_dir);
    bm25.build_from_chunks(&chunks).unwrap();

    let index = LocalSearchIndex::with_bm25(settings, bm25);
    let results = index.query("变量声明", 5, None, false).await.unwrap();
    assert!(
        !results.is_empty(),
        "LocalSearchIndex query should return results"
    );
}

#[tokio::test]
#[ignore]
async fn test_real_docs_http_app_search() {
    let (_tmp, bm25, doc_source) = build_real_index();

    let settings = real_settings(_tmp.path().to_path_buf());
    let search_index = LocalSearchIndex::with_bm25(settings, bm25);

    let docs = doc_source.load_all_documents().unwrap();
    let metadata = IndexMetadata {
        version: "test".to_string(),
        lang: "zh".to_string(),
        embedding_model: "none".to_string(),
        document_count: docs.len(),
        search_mode: "bm25".to_string(),
    };

    let app = create_http_app(search_index, doc_source, metadata);

    // Test search
    let req = Request::builder()
        .method("POST")
        .uri("/search")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"query":"仓颉语言"}"#))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let results = v["results"].as_array().unwrap();
    assert!(
        !results.is_empty(),
        "HTTP search for '仓颉语言' should return results"
    );

    // Test topics
    let req = Request::builder()
        .uri("/topics")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let categories = v["categories"].as_object().unwrap();
    assert!(
        categories.len() > 2,
        "real docs should have multiple categories"
    );
}

#[test]
#[ignore]
fn test_initialize_and_index_full_pipeline() {
    let tmp = TempDir::new().unwrap();
    let settings = real_settings(tmp.path().to_path_buf());

    let mut search_index = LocalSearchIndex::new(settings);
    let index_info = search_index.init().unwrap();

    assert!(!index_info.version.is_empty());
    assert!(
        index_info.bm25_index_dir().exists(),
        "BM25 index should exist on disk"
    );

    // Metadata file should have been written
    let metadata_path = index_info.index_dir().join("index_metadata.json");
    assert!(metadata_path.exists(), "index_metadata.json should exist");
    let meta_content = std::fs::read_to_string(&metadata_path).unwrap();
    let meta: serde_json::Value = serde_json::from_str(&meta_content).unwrap();
    assert!(meta["document_count"].as_u64().unwrap() > 0);
    assert_eq!(meta["search_mode"], "bm25");
}

#[test]
#[ignore]
fn test_initialize_and_index_is_idempotent() {
    let tmp = TempDir::new().unwrap();
    let settings = real_settings(tmp.path().to_path_buf());

    // First build
    let mut index1 = LocalSearchIndex::new(settings.clone());
    let info1 = index1.init().unwrap();

    // Second build should detect existing index and skip
    let mut index2 = LocalSearchIndex::new(settings);
    let info2 = index2.init().unwrap();

    assert_eq!(info1.version, info2.version);
}
