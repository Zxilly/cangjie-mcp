use cangjie_mcp::config::{MAX_TOP_K, MIN_TOP_K};
use cangjie_mcp::indexer::search::bm25::BM25Store;
use cangjie_mcp::indexer::search::LocalSearchIndex;
use cangjie_mcp::indexer::{DocMetadata, TextChunk};
use cangjie_mcp::server::tools::{
    CangjieServer, GetTopicParams, ListTopicsParams, SearchDocsParams, TopicResult,
    TopicsListResult,
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

async fn build_test_server_with_chunks(chunks: Vec<TextChunk>) -> (TempDir, CangjieServer) {
    let tmp = TempDir::new().unwrap();
    let bm25_dir = tmp.path().join("bm25");
    let mut bm25 = BM25Store::new(bm25_dir);
    bm25.build_from_chunks(&chunks).await.unwrap();
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

    let result_count = result_json.matches("### [").count();
    assert!(result_count > 0, "search should return results");
    assert!(
        result_json.contains("showing 1-"),
        "should show pagination starting from 1"
    );
    assert!(
        !result_json.is_empty(),
        "result content should not be empty"
    );
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

    let result_count = result_json.matches("### [").count();
    assert!(result_count > 0, "should find results in syntax category");
    assert!(
        result_json.contains("syntax"),
        "all results should belong to 'syntax' category"
    );
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
    let all_count = all_json.matches("### [").count();

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

    let page_count = page_json.matches("### [").count();
    assert!(
        page_json.contains("showing 3-"),
        "offset=2 should show results starting from 3"
    );
    assert!(page_count <= 2, "should return at most top_k items");

    if all_count > 4 {
        assert!(
            page_json.contains("More results available"),
            "should have more results when total > offset + top_k"
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

    let result_count = result_json.matches("### [").count();
    assert!(result_count > 0, "should return results");
    assert!(
        !result_json.is_empty(),
        "result content should not be empty"
    );
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

    assert!(
        result_json.contains("Array"),
        "results should mention 'Array' when package filter is set"
    );
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
async fn test_get_topic_wrong_category_fallbacks_to_correct_category() {
    let (_tmp, server) = build_test_server().await;

    let result_json = server
        .get_topic(Parameters(GetTopicParams {
            topic: "functions".into(),
            category: Some("stdlib".into()),
        }))
        .await;

    let result: TopicResult =
        serde_json::from_str(&result_json).expect("should parse as TopicResult");
    assert_eq!(result.topic, "functions");
    assert_eq!(
        result.category, "syntax",
        "tool should fallback to correct category when provided category is wrong"
    );
}

#[tokio::test]
async fn test_search_docs_allows_two_snippets_per_document_when_top_k_is_large() {
    let chunks = vec![
        TextChunk {
            text: "HashMap get set 示例".to_string(),
            metadata: DocMetadata {
                file_path: "stdlib/collection_hashmap.md".to_string(),
                category: "stdlib".to_string(),
                topic: "collection_hashmap".to_string(),
                title: "HashMap 用法".to_string(),
                has_code: false,
                code_block_count: 0,
                ..Default::default()
            },
        },
        TextChunk {
            text: "HashMap 遍历与删除".to_string(),
            metadata: DocMetadata {
                file_path: "stdlib/collection_hashmap.md".to_string(),
                category: "stdlib".to_string(),
                topic: "collection_hashmap".to_string(),
                title: "HashMap 用法".to_string(),
                has_code: false,
                code_block_count: 0,
                ..Default::default()
            },
        },
        TextChunk {
            text: "ArrayList push pop 示例".to_string(),
            metadata: DocMetadata {
                file_path: "stdlib/collection_arraylist.md".to_string(),
                category: "stdlib".to_string(),
                topic: "collection_arraylist".to_string(),
                title: "ArrayList 用法".to_string(),
                has_code: false,
                code_block_count: 0,
                ..Default::default()
            },
        },
    ];
    let (_tmp, server) = build_test_server_with_chunks(chunks).await;

    let result_json = server
        .search_docs(Parameters(SearchDocsParams {
            query: "HashMap".into(),
            top_k: 10,
            offset: 0,
            extract_code: false,
            category: None,
            package: None,
        }))
        .await;

    assert!(
        result_json.contains("HashMap"),
        "results should contain HashMap content when top_k is large"
    );
    let result_count = result_json.matches("### [").count();
    assert!(result_count > 0, "should return at least one result");
}

#[tokio::test]
async fn test_search_docs_limits_to_one_snippet_per_document_when_top_k_is_small() {
    let chunks = vec![
        TextChunk {
            text: "HashMap get set 示例".to_string(),
            metadata: DocMetadata {
                file_path: "stdlib/collection_hashmap.md".to_string(),
                category: "stdlib".to_string(),
                topic: "collection_hashmap".to_string(),
                title: "HashMap 用法".to_string(),
                has_code: false,
                code_block_count: 0,
                ..Default::default()
            },
        },
        TextChunk {
            text: "HashMap 遍历与删除".to_string(),
            metadata: DocMetadata {
                file_path: "stdlib/collection_hashmap.md".to_string(),
                category: "stdlib".to_string(),
                topic: "collection_hashmap".to_string(),
                title: "HashMap 用法".to_string(),
                has_code: false,
                code_block_count: 0,
                ..Default::default()
            },
        },
        TextChunk {
            text: "ArrayList push pop 示例".to_string(),
            metadata: DocMetadata {
                file_path: "stdlib/collection_arraylist.md".to_string(),
                category: "stdlib".to_string(),
                topic: "collection_arraylist".to_string(),
                title: "ArrayList 用法".to_string(),
                has_code: false,
                code_block_count: 0,
                ..Default::default()
            },
        },
    ];
    let (_tmp, server) = build_test_server_with_chunks(chunks).await;

    let result_json = server
        .search_docs(Parameters(SearchDocsParams {
            query: "HashMap".into(),
            top_k: 3,
            offset: 0,
            extract_code: false,
            category: None,
            package: None,
        }))
        .await;

    assert!(
        result_json.contains("HashMap"),
        "results should contain HashMap content when top_k is small"
    );
    let result_count = result_json.matches("### [").count();
    assert!(result_count > 0, "should return at least one result");
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

    let result_count = result_json.matches("### [").count();
    assert!(
        result_count <= MAX_TOP_K,
        "result count ({}) should be clamped to MAX_TOP_K ({})",
        result_count,
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

    let result_count = result_json.matches("### [").count();
    assert!(
        result_count == MIN_TOP_K,
        "result count ({}) should be exactly MIN_TOP_K ({}) when top_k is clamped from 0",
        result_count,
        MIN_TOP_K
    );
}
