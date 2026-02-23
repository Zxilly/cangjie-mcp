use axum::body::Body;
use axum::http::{Request, StatusCode};
use cangjie_mcp::indexer::search::bm25::BM25Store;
use cangjie_mcp::indexer::search::LocalSearchIndex;
use cangjie_mcp::indexer::IndexMetadata;
use cangjie_mcp::server::http::create_http_app;
use cangjie_mcp_test::{sample_chunks, sample_documents, test_settings, MockDocumentSource};
use http_body_util::BodyExt;
use tempfile::TempDir;
use tower::ServiceExt;

/// Build a fully-wired HTTP app backed by real BM25 + MockDocumentSource.
async fn build_test_app() -> (TempDir, axum::Router) {
    let tmp = TempDir::new().unwrap();
    let bm25_dir = tmp.path().join("bm25_index");

    let mut bm25 = BM25Store::new(bm25_dir);
    bm25.build_from_chunks(&sample_chunks()).await.unwrap();

    let settings = test_settings(tmp.path().to_path_buf());
    let search_index = LocalSearchIndex::with_bm25(settings, bm25).await;

    let docs = sample_documents();
    let doc_source = Box::new(MockDocumentSource::from_docs(&docs));

    let metadata = IndexMetadata {
        version: "test".to_string(),
        lang: "zh".to_string(),
        embedding_model: "none".to_string(),
        document_count: docs.len(),
        search_mode: "bm25".to_string(),
    };

    let app = create_http_app(search_index, doc_source, metadata).await;
    (tmp, app)
}

async fn get(app: axum::Router, uri: &str) -> (StatusCode, String) {
    let req = Request::builder().uri(uri).body(Body::empty()).unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let status = resp.status();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&body).to_string())
}

async fn post_json(app: axum::Router, uri: &str, json: &str) -> (StatusCode, String) {
    let req = Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(json.to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let status = resp.status();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&body).to_string())
}

#[tokio::test]
async fn test_info_endpoint() {
    let (_tmp, app) = build_test_app().await;
    let (status, body) = get(app, "/info").await;
    assert_eq!(status, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["version"], "test");
    assert_eq!(v["lang"], "zh");
    assert!(v["document_count"].as_u64().unwrap() > 0);
}

#[tokio::test]
async fn test_search_endpoint() {
    let (_tmp, app) = build_test_app().await;
    let (status, body) = post_json(app, "/search", r#"{"query":"函数"}"#).await;
    assert_eq!(status, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    let results = v["results"].as_array().unwrap();
    assert!(!results.is_empty(), "search should return results");
}

#[tokio::test]
async fn test_search_empty_query() {
    let (_tmp, app) = build_test_app().await;
    let (status, _body) = post_json(app, "/search", r#"{"query":""}"#).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_topics_endpoint() {
    let (_tmp, app) = build_test_app().await;
    let (status, body) = get(app, "/topics").await;
    assert_eq!(status, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    let categories = v["categories"].as_object().unwrap();
    assert!(!categories.is_empty(), "categories should not be empty");
    assert!(categories.contains_key("syntax"));
    assert!(categories.contains_key("stdlib"));
    assert!(categories.contains_key("cjpm"));
}

#[tokio::test]
async fn test_topic_detail() {
    let (_tmp, app) = build_test_app().await;
    let (status, body) = get(app, "/topics/syntax/functions").await;
    assert_eq!(status, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert!(v["content"].as_str().unwrap().contains("函数"));
    assert_eq!(v["category"], "syntax");
    assert_eq!(v["topic"], "functions");
}

#[tokio::test]
async fn test_topic_not_found() {
    let (_tmp, app) = build_test_app().await;
    let (status, body) = get(app, "/topics/nonexistent/fake").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert!(v["error"].as_str().is_some());
}

#[tokio::test]
async fn test_search_whitespace_only_query() {
    let (_tmp, app) = build_test_app().await;
    let (status, body) = post_json(app, "/search", r#"{"query":"   "}"#).await;
    assert_eq!(status, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    let results = v["results"].as_array().unwrap();
    assert!(
        results.is_empty(),
        "whitespace-only query should return no results"
    );
}

#[tokio::test]
async fn test_search_with_category_filter() {
    let (_tmp, app) = build_test_app().await;
    let (status, body) = post_json(app, "/search", r#"{"query":"函数","category":"stdlib"}"#).await;
    assert_eq!(status, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    let results = v["results"].as_array().unwrap();
    for r in results {
        assert_eq!(
            r["metadata"]["category"].as_str().unwrap(),
            "stdlib",
            "category filter should be applied"
        );
    }
}

#[tokio::test]
async fn test_search_custom_top_k() {
    let (_tmp, app) = build_test_app().await;
    let (status, body) = post_json(app, "/search", r#"{"query":"仓颉","top_k":2}"#).await;
    assert_eq!(status, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    let results = v["results"].as_array().unwrap();
    assert!(
        results.len() <= 2,
        "top_k=2 should return at most 2 results, got {}",
        results.len()
    );
}

#[tokio::test]
async fn test_search_result_metadata_fields() {
    let (_tmp, app) = build_test_app().await;
    let (status, body) = post_json(app, "/search", r#"{"query":"函数"}"#).await;
    assert_eq!(status, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    let results = v["results"].as_array().unwrap();
    assert!(!results.is_empty());

    let first = &results[0];
    assert!(first["text"].is_string());
    assert!(first["score"].is_f64());
    assert!(first["metadata"]["file_path"].is_string());
    assert!(first["metadata"]["category"].is_string());
    assert!(first["metadata"]["topic"].is_string());
    assert!(first["metadata"]["title"].is_string());
    assert!(first["metadata"]["has_code"].is_boolean());
}

#[tokio::test]
async fn test_topic_detail_response_fields() {
    let (_tmp, app) = build_test_app().await;
    let (status, body) = get(app, "/topics/cjpm/getting_started").await;
    assert_eq!(status, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert!(v["content"].as_str().unwrap().contains("CJPM"));
    assert_eq!(v["category"], "cjpm");
    assert_eq!(v["topic"], "getting_started");
    assert!(!v["file_path"].as_str().unwrap().is_empty());
    assert!(!v["title"].as_str().unwrap().is_empty());
}

#[tokio::test]
async fn test_search_invalid_json() {
    let (_tmp, app) = build_test_app().await;
    let req = axum::http::Request::builder()
        .method("POST")
        .uri("/search")
        .header("content-type", "application/json")
        .body(Body::from("not valid json"))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert!(
        resp.status().is_client_error(),
        "invalid JSON should return 4xx, got {}",
        resp.status()
    );
}
