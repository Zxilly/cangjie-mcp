use cangjie_mcp::config::{MAX_TOP_K, MIN_TOP_K};
use cangjie_mcp::indexer::search::bm25::BM25Store;
use cangjie_mcp::indexer::search::LocalSearchIndex;
use cangjie_mcp::server::tools::{
    CangjieServer, DocsSearchResult, GetTopicParams, ListTopicsParams, SearchDocsParams,
    TopicResult, TopicsListResult,
};
use cangjie_mcp::Parameters;
use cangjie_mcp_test::{sample_chunks, sample_documents, test_settings, MockDocumentSource};
use tempfile::TempDir;

async fn build_test_server() -> (TempDir, CangjieServer) {
    let tmp = TempDir::new().unwrap();
    let bm25_dir = tmp.path().join("bm25");
    let mut bm25 = BM25Store::new(bm25_dir);
    bm25.build_from_chunks(&sample_chunks()).await.unwrap();
    let settings = test_settings(tmp.path().to_path_buf());
    let docs = sample_documents();
    let source = Box::new(MockDocumentSource::from_docs(&docs));
    let search = LocalSearchIndex::with_bm25(settings.clone(), bm25).await;
    let server = CangjieServer::with_local_state(settings, search, source);
    (tmp, server)
}

#[tokio::test]
async fn test_search_docs_basic() {
    let (_tmp, server) = build_test_server().await;

    let result_json = server
        .search_docs(Parameters(SearchDocsParams {
            query: "函数".into(),
            top_k: 5,
            offset: 0,
            extract_code: false,
            category: None,
            package: None,
        }))
        .await;

    let result: DocsSearchResult =
        serde_json::from_str(&result_json).expect("should parse as DocsSearchResult");

    assert!(!result.items.is_empty(), "search should return results");
    assert!(result.count > 0);
    assert_eq!(result.count, result.items.len());
    assert_eq!(result.offset, 0);

    for item in &result.items {
        assert!(!item.content.is_empty(), "content should not be empty");
        assert!(!item.file_path.is_empty(), "file_path should not be empty");
        assert!(!item.category.is_empty(), "category should not be empty");
        assert!(!item.topic.is_empty(), "topic should not be empty");
        assert!(item.score > 0.0, "score should be positive");
    }
}

#[tokio::test]
async fn test_search_docs_with_category() {
    let (_tmp, server) = build_test_server().await;

    let result_json = server
        .search_docs(Parameters(SearchDocsParams {
            query: "函数".into(),
            top_k: 10,
            offset: 0,
            extract_code: false,
            category: Some("syntax".into()),
            package: None,
        }))
        .await;

    let result: DocsSearchResult =
        serde_json::from_str(&result_json).expect("should parse as DocsSearchResult");

    assert!(
        !result.items.is_empty(),
        "should find results in syntax category"
    );
    for item in &result.items {
        assert_eq!(
            item.category, "syntax",
            "all results should belong to 'syntax' category"
        );
    }
}

#[tokio::test]
async fn test_search_docs_pagination() {
    let (_tmp, server) = build_test_server().await;

    let all_json = server
        .search_docs(Parameters(SearchDocsParams {
            query: "仓颉".into(),
            top_k: 20,
            offset: 0,
            extract_code: false,
            category: None,
            package: None,
        }))
        .await;
    let all: DocsSearchResult = serde_json::from_str(&all_json).unwrap();
    let total = all.total;

    let page_json = server
        .search_docs(Parameters(SearchDocsParams {
            query: "仓颉".into(),
            top_k: 2,
            offset: 2,
            extract_code: false,
            category: None,
            package: None,
        }))
        .await;
    let page: DocsSearchResult = serde_json::from_str(&page_json).unwrap();

    assert_eq!(page.offset, 2, "offset should be 2");
    assert!(page.count <= 2, "should return at most top_k items");

    if total > 4 {
        assert!(
            page.has_more,
            "should have more results when total > offset + top_k"
        );
        assert!(
            page.next_offset.is_some(),
            "next_offset should be present when has_more is true"
        );
    }
}

#[tokio::test]
async fn test_search_docs_extract_code() {
    let (_tmp, server) = build_test_server().await;

    let result_json = server
        .search_docs(Parameters(SearchDocsParams {
            query: "函数定义".into(),
            top_k: 5,
            offset: 0,
            extract_code: true,
            category: None,
            package: None,
        }))
        .await;

    let result: DocsSearchResult =
        serde_json::from_str(&result_json).expect("should parse as DocsSearchResult");

    assert!(!result.items.is_empty(), "should return results");

    let has_code_item = result.items.iter().any(|item| item.code_examples.is_some());
    assert!(
        has_code_item,
        "at least one result should have code_examples when extract_code is true"
    );

    for item in &result.items {
        if let Some(ref examples) = item.code_examples {
            for ex in examples {
                assert!(!ex.code.is_empty(), "code example code should not be empty");
                assert!(
                    !ex.source_topic.is_empty(),
                    "code example source_topic should not be empty"
                );
            }
        }
    }
}

#[tokio::test]
async fn test_search_docs_package_filter() {
    let (_tmp, server) = build_test_server().await;

    let result_json = server
        .search_docs(Parameters(SearchDocsParams {
            query: "集合".into(),
            top_k: 10,
            offset: 0,
            extract_code: false,
            category: None,
            package: Some("Array".into()),
        }))
        .await;

    let result: DocsSearchResult =
        serde_json::from_str(&result_json).expect("should parse as DocsSearchResult");

    for item in &result.items {
        assert!(
            item.content.contains("Array"),
            "all results should mention 'Array' when package filter is set, got: {}",
            &item.content[..item.content.len().min(100)]
        );
    }
}

#[tokio::test]
async fn test_get_topic_found() {
    let (_tmp, server) = build_test_server().await;

    let result_json = server
        .get_topic(Parameters(GetTopicParams {
            topic: "functions".into(),
            category: Some("syntax".into()),
        }))
        .await;

    let result: TopicResult =
        serde_json::from_str(&result_json).expect("should parse as TopicResult");

    assert!(
        result.content.contains("函数"),
        "topic content should contain '函数'"
    );
    assert_eq!(result.category, "syntax");
    assert_eq!(result.topic, "functions");
    assert!(!result.title.is_empty(), "title should not be empty");
    assert!(
        !result.file_path.is_empty(),
        "file_path should not be empty"
    );
}

#[tokio::test]
async fn test_get_topic_not_found_with_suggestions() {
    let (_tmp, server) = build_test_server().await;

    let result = server
        .get_topic(Parameters(GetTopicParams {
            topic: "functons".into(), // typo: missing 'i'
            category: None,
        }))
        .await;

    // Should not parse as TopicResult (it's a plain error message)
    assert!(
        result.contains("not found"),
        "response should indicate topic not found, got: {result}"
    );
    assert!(
        result.contains("Did you mean"),
        "response should contain suggestions, got: {result}"
    );
    assert!(
        result.contains("functions"),
        "suggestions should include 'functions', got: {result}"
    );
}

#[tokio::test]
async fn test_list_topics_all() {
    let (_tmp, server) = build_test_server().await;

    let result_json = server
        .list_topics(Parameters(ListTopicsParams { category: None }))
        .await;

    let result: TopicsListResult =
        serde_json::from_str(&result_json).expect("should parse as TopicsListResult");

    assert!(result.error.is_none(), "should not have an error");
    assert!(
        result.categories.contains_key("syntax"),
        "should contain 'syntax' category"
    );
    assert!(
        result.categories.contains_key("stdlib"),
        "should contain 'stdlib' category"
    );
    assert!(
        result.categories.contains_key("cjpm"),
        "should contain 'cjpm' category"
    );
    assert!(
        result.total_categories >= 3,
        "should have at least 3 categories"
    );
    assert!(result.total_topics > 0, "should have some topics");
}

#[tokio::test]
async fn test_list_topics_filter_category() {
    let (_tmp, server) = build_test_server().await;

    let result_json = server
        .list_topics(Parameters(ListTopicsParams {
            category: Some("syntax".into()),
        }))
        .await;

    let result: TopicsListResult =
        serde_json::from_str(&result_json).expect("should parse as TopicsListResult");

    assert!(result.error.is_none(), "should not have an error");
    assert_eq!(
        result.total_categories, 1,
        "should have exactly 1 category when filtered"
    );
    assert!(
        result.categories.contains_key("syntax"),
        "should contain 'syntax' category"
    );
    assert!(
        !result.categories.contains_key("stdlib"),
        "should not contain 'stdlib' when filtered by 'syntax'"
    );
    assert!(
        !result.categories.contains_key("cjpm"),
        "should not contain 'cjpm' when filtered by 'syntax'"
    );

    let syntax_topics = &result.categories["syntax"];
    let topic_names: Vec<&str> = syntax_topics.iter().map(|t| t.name.as_str()).collect();
    assert!(
        topic_names.contains(&"functions"),
        "syntax category should contain 'functions' topic"
    );
}

#[tokio::test]
async fn test_list_topics_invalid_category() {
    let (_tmp, server) = build_test_server().await;

    let result_json = server
        .list_topics(Parameters(ListTopicsParams {
            category: Some("nonexistent".into()),
        }))
        .await;

    let result: TopicsListResult =
        serde_json::from_str(&result_json).expect("should parse as TopicsListResult");

    assert!(
        result.error.is_some(),
        "should have an error for nonexistent category"
    );
    assert!(
        result.error.as_ref().unwrap().contains("not found"),
        "error should mention category not found"
    );
    assert!(
        result.available_categories.is_some(),
        "should list available categories"
    );
    let available = result.available_categories.unwrap();
    assert!(
        available.contains(&"syntax".to_string()),
        "available categories should include 'syntax'"
    );
}

#[tokio::test]
async fn test_search_docs_not_initialized() {
    let tmp = TempDir::new().unwrap();
    let settings = test_settings(tmp.path().to_path_buf());
    let server = CangjieServer::new(settings);

    let result = server
        .search_docs(Parameters(SearchDocsParams {
            query: "函数".into(),
            top_k: 5,
            offset: 0,
            extract_code: false,
            category: None,
            package: None,
        }))
        .await;

    assert!(
        result.contains("not initialized"),
        "should report server not initialized, got: {result}"
    );
}

#[tokio::test]
async fn test_search_docs_max_top_k_clamped() {
    let (_tmp, server) = build_test_server().await;

    let result_json = server
        .search_docs(Parameters(SearchDocsParams {
            query: "仓颉".into(),
            top_k: 999,
            offset: 0,
            extract_code: false,
            category: None,
            package: None,
        }))
        .await;

    let result: DocsSearchResult =
        serde_json::from_str(&result_json).expect("should parse as DocsSearchResult");

    assert!(
        result.count <= MAX_TOP_K,
        "result count ({}) should be clamped to MAX_TOP_K ({})",
        result.count,
        MAX_TOP_K
    );
}

#[tokio::test]
async fn test_search_docs_min_top_k_clamped() {
    let (_tmp, server) = build_test_server().await;

    let result_json = server
        .search_docs(Parameters(SearchDocsParams {
            query: "函数".into(),
            top_k: 0,
            offset: 0,
            extract_code: false,
            category: None,
            package: None,
        }))
        .await;

    let result: DocsSearchResult =
        serde_json::from_str(&result_json).expect("should parse as DocsSearchResult");

    assert!(
        result.count >= MIN_TOP_K && result.count <= MIN_TOP_K,
        "result count ({}) should be exactly MIN_TOP_K ({}) when top_k is clamped from 0",
        result.count,
        MIN_TOP_K
    );
}
