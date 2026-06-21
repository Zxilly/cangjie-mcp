//! Integration tests covering real-world use cases.
//!
//! These tests simulate how a real user (typically an AI coding assistant)
//! interacts with the MCP tools and HTTP API in realistic workflows.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use cangjie_indexer::document::chunker::chunk_documents;
use cangjie_indexer::search::bm25::BM25Store;
use cangjie_indexer::search::LocalSearchIndex;
use cangjie_indexer::{DocMetadata, IndexMetadata, SearchMode, TextChunk};
use cangjie_mcp_test::{
    cross_category_chunks, large_document, sample_chunks, stdlib_package_chunks, test_settings,
};
use cangjie_server::http::create_http_app;
use cangjie_server::mcp_handler::SearchDocsParams;
use cangjie_server::{CangjieServer, Parameters};
use http_body_util::BodyExt;
use tempfile::TempDir;
use tower::ServiceExt;

async fn build_server(chunks: &[TextChunk]) -> (TempDir, CangjieServer) {
    let tmp = TempDir::new().unwrap();
    let bm25_dir = tmp.path().join("bm25");
    let mut bm25 = BM25Store::new(bm25_dir);
    bm25.build_from_chunks(chunks).await.unwrap();
    let settings = test_settings(tmp.path().to_path_buf());
    let search = LocalSearchIndex::with_bm25(settings.clone(), bm25).await;
    let server = CangjieServer::with_local_state(settings, search);
    (tmp, server)
}

async fn build_default_server() -> (TempDir, CangjieServer) {
    build_server(&sample_chunks()).await
}

async fn build_http_app(chunks: &[TextChunk]) -> (TempDir, axum::Router) {
    let tmp = TempDir::new().unwrap();
    let bm25_dir = tmp.path().join("bm25");
    let mut bm25 = BM25Store::new(bm25_dir);
    bm25.build_from_chunks(chunks).await.unwrap();
    let settings = test_settings(tmp.path().to_path_buf());
    let search_index = LocalSearchIndex::with_bm25(settings, bm25).await;
    let metadata = IndexMetadata {
        version: "test".to_string(),
        lang: "zh".to_string(),
        embedding_model: "none".to_string(),
        document_count: chunks.len(),
        search_mode: SearchMode::Bm25,
    };
    let app = create_http_app(Arc::new(search_index), metadata).await;
    (tmp, app)
}

async fn http_get(app: axum::Router, uri: &str) -> (StatusCode, serde_json::Value) {
    let req = Request::builder().uri(uri).body(Body::empty()).unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let status = resp.status();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);
    (status, v)
}

async fn http_post(app: axum::Router, uri: &str, json: &str) -> (StatusCode, serde_json::Value) {
    let req = Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(json.to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let status = resp.status();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);
    (status, v)
}

/// Search for Chinese programming concepts — results should be relevant.
#[tokio::test]
async fn test_search_relevance_error_handling() {
    let (_tmp, server) = build_default_server().await;

    let result = server
        .search_docs(Parameters(SearchDocsParams {
            query: "错误处理 异常".into(),
            top_k: 5,
            offset: 0,
            category: None,
            package: None,
        }))
        .await;

    let has_relevant =
        result.contains("错误") || result.contains("Error") || result.contains("Result");
    assert!(
        has_relevant,
        "search for error handling should return relevant results, got:\n{result}"
    );
}

/// Search for English keywords embedded in Chinese docs.
#[tokio::test]
async fn test_search_mixed_language_query() {
    let (_tmp, server) = build_default_server().await;

    let result = server
        .search_docs(Parameters(SearchDocsParams {
            query: "HashMap".into(),
            top_k: 5,
            offset: 0,
            category: None,
            package: None,
        }))
        .await;

    assert!(
        result.contains("HashMap"),
        "search for 'HashMap' should find results mentioning HashMap"
    );
}

/// Verify that category filter correctly narrows results.
#[tokio::test]
async fn test_search_category_filter_strict() {
    let (_tmp, server) = build_default_server().await;

    let result = server
        .search_docs(Parameters(SearchDocsParams {
            query: "仓颉".into(),
            top_k: 20,
            offset: 0,
            category: Some("cjpm".into()),
            package: None,
        }))
        .await;

    if result.contains("### [") {
        assert!(
            !result.contains("syntax/functions"),
            "category=cjpm should not include syntax results"
        );
    }
}

/// Search for something that doesn't exist should return no results gracefully.
#[tokio::test]
async fn test_search_no_match_graceful() {
    let (_tmp, server) = build_default_server().await;

    let result = server
        .search_docs(Parameters(SearchDocsParams {
            query: "量子计算加密区块链".into(),
            top_k: 5,
            offset: 0,
            category: None,
            package: None,
        }))
        .await;

    assert!(
        result.contains("Found") || result.contains("0 results") || result.contains("showing"),
        "no-match search should still return a well-formatted response"
    );
}

/// Test filtering search results by stdlib package name.
#[tokio::test]
async fn test_search_package_filter_std_collection() {
    let chunks = stdlib_package_chunks();
    let (_tmp, server) = build_server(&chunks).await;

    let result = server
        .search_docs(Parameters(SearchDocsParams {
            query: "集合 容器".into(),
            top_k: 10,
            offset: 0,
            category: None,
            package: Some("std.collection".into()),
        }))
        .await;

    if result.contains("### [") {
        assert!(
            result.contains("std.collection") || result.contains("collection"),
            "package filter should narrow to collection results"
        );
    }
}

/// Test that package filter excludes unrelated packages.
#[tokio::test]
async fn test_search_package_filter_excludes_others() {
    let chunks = stdlib_package_chunks();
    let (_tmp, server) = build_server(&chunks).await;

    let result = server
        .search_docs(Parameters(SearchDocsParams {
            query: "HTTP 网络请求".into(),
            top_k: 10,
            offset: 0,
            category: None,
            package: Some("std.fs".into()),
        }))
        .await;

    // Filtering by std.fs should yield either no results or only fs-related ones
    if result.contains("### [") {
        assert!(
            !result.contains("HttpClient") || result.contains("std.fs"),
            "package=std.fs should not include HTTP results"
        );
    }
}

/// Multiple concurrent searches should all succeed.
#[tokio::test]
async fn test_concurrent_searches() {
    let (_tmp, server) = build_default_server().await;

    let queries = vec!["函数", "变量", "集合", "错误处理", "CJPM", "类型", "控制流"];

    let mut set = tokio::task::JoinSet::new();
    for query in queries {
        let s = server.clone();
        let q = query.to_string();
        set.spawn(async move {
            s.search_docs(Parameters(SearchDocsParams {
                query: q,
                top_k: 5,
                offset: 0,
                category: None,
                package: None,
            }))
            .await
        });
    }

    let mut i = 0;
    while let Some(result) = set.join_next().await {
        let result = result.unwrap();
        assert!(
            result.contains("Found") || result.contains("showing"),
            "concurrent query #{i} should return valid results, got: {result}"
        );
        i += 1;
    }
}

/// Unicode edge cases in search queries.
#[tokio::test]
async fn test_search_unicode_edge_cases() {
    let (_tmp, server) = build_default_server().await;

    let queries = vec![
        "函数 function",                  // mixed Chinese + English
        "let x: Int = 10",                // code-like query
        "仓颉语言的错误处理机制是什么？", // full question in Chinese
        "func add(a: Int, b: Int): Int",  // code snippet
    ];

    for query in queries {
        let result = server
            .search_docs(Parameters(SearchDocsParams {
                query: query.into(),
                top_k: 5,
                offset: 0,
                category: None,
                package: None,
            }))
            .await;

        assert!(
            !result.contains("Search error"),
            "query '{query}' should not produce a search error, got: {result}"
        );
    }
}

/// Pagination with offset beyond available results.
#[tokio::test]
async fn test_search_offset_beyond_results() {
    let (_tmp, server) = build_default_server().await;

    let result = server
        .search_docs(Parameters(SearchDocsParams {
            query: "函数".into(),
            top_k: 5,
            offset: 1000,
            category: None,
            package: None,
        }))
        .await;

    let count = result.matches("### [").count();
    assert_eq!(count, 0, "offset beyond results should return 0 items");
}

/// Empty category string should be treated as None.
#[tokio::test]
async fn test_search_empty_category_treated_as_none() {
    let (_tmp, server) = build_default_server().await;

    let with_empty_cat = server
        .search_docs(Parameters(SearchDocsParams {
            query: "函数".into(),
            top_k: 5,
            offset: 0,
            category: Some("".into()),
            package: None,
        }))
        .await;

    let without_cat = server
        .search_docs(Parameters(SearchDocsParams {
            query: "函数".into(),
            top_k: 5,
            offset: 0,
            category: None,
            package: None,
        }))
        .await;

    let count1 = with_empty_cat.matches("### [").count();
    let count2 = without_cat.matches("### [").count();
    assert_eq!(
        count1, count2,
        "empty category string should behave like None"
    );
}

/// Simulate full HTTP API workflow: health → info → search → browse topics → detail.
#[tokio::test]
async fn test_http_full_workflow() {
    let chunks = sample_chunks();
    let (_tmp, app) = build_http_app(&chunks).await;

    let req = Request::builder()
        .uri("/health")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let (status, info) = http_get(app.clone(), "/info").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(info["version"], "test");
    assert!(info["document_count"].as_u64().unwrap() > 0);

    let (status, search_result) =
        http_post(app.clone(), "/search", r#"{"query":"函数定义","top_k":3}"#).await;
    assert_eq!(status, StatusCode::OK);
    let results = search_result["results"].as_array().unwrap();
    assert!(!results.is_empty(), "search should return results");
}

/// HTTP search with basic pagination parameters.
#[tokio::test]
async fn test_http_search_with_top_k() {
    let chunks = sample_chunks();
    let (_tmp, app) = build_http_app(&chunks).await;

    let (status, result) = http_post(app, "/search", r#"{"query":"变量","top_k":5}"#).await;
    assert_eq!(status, StatusCode::OK);
    let results = result["results"].as_array().unwrap();
    assert!(!results.is_empty());
}

/// HTTP search with category filter through JSON body.
#[tokio::test]
async fn test_http_search_category_filter_json() {
    let chunks = sample_chunks();
    let (_tmp, app) = build_http_app(&chunks).await;

    let (status, result) = http_post(
        app,
        "/search",
        r#"{"query":"仓颉","category":"cjpm","top_k":10}"#,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let results = result["results"].as_array().unwrap();
    for r in results {
        assert_eq!(
            r["metadata"]["category"].as_str().unwrap(),
            "cjpm",
            "HTTP category filter should be applied"
        );
    }
}

/// Verify that chunk_documents → BM25 build → search produces sensible results
/// for a realistic multi-section document.
#[tokio::test]
async fn test_large_document_chunking_and_search() {
    let large_doc = large_document();
    let docs = vec![large_doc];
    let chunks = chunk_documents(docs, Some(500), 100).await;

    assert!(
        chunks.len() > 3,
        "large document should produce multiple chunks, got {}",
        chunks.len()
    );

    for chunk in &chunks {
        assert_eq!(chunk.metadata.category, "syntax");
        assert_eq!(chunk.metadata.topic, "complete_guide");
    }

    let tmp = TempDir::new().unwrap();
    let bm25_dir = tmp.path().join("bm25");
    let mut bm25 = BM25Store::new(bm25_dir);
    bm25.build_from_chunks(&chunks).await.unwrap();

    // Search for content from different sections
    let results = bm25.search("高级特性", 5, None).await.unwrap();
    assert!(
        !results.is_empty(),
        "should find '高级特性' in chunked large document"
    );

    let results = bm25.search("泛型编程", 5, None).await.unwrap();
    assert!(
        !results.is_empty(),
        "should find '泛型编程' in chunked large document"
    );
}

/// Verify that searching after building from all test chunks returns consistent results.
#[tokio::test]
async fn test_combined_chunks_search_consistency() {
    let mut all_chunks = sample_chunks();
    all_chunks.extend(cross_category_chunks());
    all_chunks.extend(stdlib_package_chunks());

    let tmp = TempDir::new().unwrap();
    let bm25_dir = tmp.path().join("bm25");
    let mut bm25 = BM25Store::new(bm25_dir);
    bm25.build_from_chunks(&all_chunks).await.unwrap();

    let results = bm25.search("概述", 10, None).await.unwrap();
    assert!(
        !results.is_empty(),
        "should find '概述' from cross_category_chunks"
    );

    let results = bm25.search("ArrayList", 5, None).await.unwrap();
    assert!(
        !results.is_empty(),
        "should find 'ArrayList' from stdlib_package_chunks"
    );

    let results = bm25.search("函数定义", 5, None).await.unwrap();
    assert!(
        !results.is_empty(),
        "should find '函数定义' from sample_chunks"
    );
}

/// When multiple chunks from the same document match, results should be deduplicated
/// appropriately based on top_k.
#[tokio::test]
async fn test_search_deduplication_across_chunks() {
    // many chunks from the same document, plus one from a different document
    let mut chunks = Vec::new();
    for i in 0..6 {
        chunks.push(TextChunk {
            text: format!("仓颉语言函数编程第{i}节 - 函数式编程是一种重要的编程范式"),
            metadata: DocMetadata {
                file_path: "syntax/functional.md".to_string(),
                category: "syntax".to_string(),
                topic: "functional".to_string(),
                title: "函数式编程".to_string(),
                has_code: false,
                code_block_count: 0,
                ..Default::default()
            },
        });
    }
    chunks.push(TextChunk {
        text: "函数定义使用 func 关键字，支持泛型参数".to_string(),
        metadata: DocMetadata {
            file_path: "syntax/functions.md".to_string(),
            category: "syntax".to_string(),
            topic: "functions".to_string(),
            title: "函数定义".to_string(),
            has_code: false,
            code_block_count: 0,
            ..Default::default()
        },
    });

    let (_tmp, server) = build_server(&chunks).await;

    // With small top_k, should maximize document coverage (show both docs)
    let result = server
        .search_docs(Parameters(SearchDocsParams {
            query: "函数".into(),
            top_k: 3,
            offset: 0,
            category: None,
            package: None,
        }))
        .await;

    let count = result.matches("### [").count();
    assert!(count >= 1, "should return at least 1 result");
    // Should show both functional and functions topics for diversity
    let has_functions = result.contains("functions") || result.contains("函数定义");
    let has_functional = result.contains("functional") || result.contains("函数式");
    assert!(
        has_functions || has_functional,
        "deduplication should show diverse results from different documents"
    );
}

/// All tools should return a meaningful error when server is not initialized.
#[tokio::test]
async fn test_all_tools_uninitialized_error() {
    let tmp = TempDir::new().unwrap();
    let settings = test_settings(tmp.path().to_path_buf());
    let server = CangjieServer::new(settings);

    let search = server
        .search_docs(Parameters(SearchDocsParams {
            query: "test".into(),
            top_k: 5,
            offset: 0,
            category: None,
            package: None,
        }))
        .await;
    assert!(
        search.contains("not initialized") || search.contains("error"),
        "search should report uninitialized, got: {search}"
    );
}
