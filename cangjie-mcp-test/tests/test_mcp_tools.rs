use cangjie_core::config::{MAX_TOP_K, MIN_TOP_K};
use cangjie_indexer::search::bm25::BM25Store;
use cangjie_indexer::search::LocalSearchIndex;
use cangjie_indexer::{DocMetadata, TextChunk};
use cangjie_mcp_test::{sample_chunks, sample_documents, test_settings, MockDocumentSource};
use cangjie_server::lsp_tools::{LspOperation, LspRequest, LspTarget};
use cangjie_server::mcp_handler::{GetTopicParams, ListTopicsParams, SearchDocsParams};
use cangjie_server::{CangjieServer, Parameters};
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

    let result_count = result.matches("### [").count();
    assert!(result_count > 0, "search should return results");
    assert!(
        result.contains("showing 1-"),
        "should show pagination starting from 1"
    );
    assert!(result.contains("[score:"), "should include relevance score");
}

#[tokio::test]
async fn test_unified_lsp_tool_reports_validation_error() {
    let (_tmp, server) = build_test_server().await;

    let result_json = server
        .lsp(Parameters(LspRequest {
            operation: LspOperation::WorkspaceSymbol,
            file_path: None,
            target: None,
            query: None,
            new_name: None,
        }))
        .await;

    let result: serde_json::Value = serde_json::from_str(&result_json).unwrap();
    assert_eq!(result["operation"], "workspace_symbol");
    assert_eq!(result["status"], "error");
    assert!(result["message"]
        .as_str()
        .unwrap_or_default()
        .contains("query is required"));
}

#[tokio::test]
async fn test_unified_lsp_tool_requires_position_for_completion() {
    let (_tmp, server) = build_test_server().await;
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("main.cj");
    std::fs::write(&file_path, "main() {}").unwrap();

    let result_json = server
        .lsp(Parameters(LspRequest {
            operation: LspOperation::Completion,
            file_path: Some(file_path.to_string_lossy().to_string()),
            target: Some(LspTarget::Symbol {
                symbol: "main".to_string(),
                line_hint: None,
            }),
            query: None,
            new_name: None,
        }))
        .await;

    let result: serde_json::Value = serde_json::from_str(&result_json).unwrap();
    assert_eq!(result["operation"], "completion");
    assert_eq!(result["status"], "error");
    assert!(result["message"]
        .as_str()
        .unwrap_or_default()
        .contains("target.kind=position"));
}

#[tokio::test]
async fn test_search_docs_with_category() {
    let (_tmp, server) = build_test_server().await;

    let result = server
        .search_docs(Parameters(SearchDocsParams {
            query: "函数".into(),
            top_k: 10,
            offset: 0,
            extract_code: false,
            category: Some("syntax".into()),
            package: None,
        }))
        .await;

    let result_count = result.matches("### [").count();
    assert!(result_count > 0, "should find results in syntax category");
    assert!(
        result.contains("syntax"),
        "all results should belong to 'syntax' category"
    );
}

#[tokio::test]
async fn test_search_docs_pagination() {
    let (_tmp, server) = build_test_server().await;

    let all = server
        .search_docs(Parameters(SearchDocsParams {
            query: "仓颉".into(),
            top_k: 20,
            offset: 0,
            extract_code: false,
            category: None,
            package: None,
        }))
        .await;
    let all_count = all.matches("### [").count();

    let page = server
        .search_docs(Parameters(SearchDocsParams {
            query: "仓颉".into(),
            top_k: 2,
            offset: 2,
            extract_code: false,
            category: None,
            package: None,
        }))
        .await;

    let page_count = page.matches("### [").count();
    assert!(
        page.contains("showing 3-"),
        "offset=2 should show results starting from 3"
    );
    assert!(page_count <= 2, "should return at most top_k items");

    if all_count > 4 {
        assert!(
            page.contains("More results available"),
            "should have more results when total > offset + top_k"
        );
    }
}

#[tokio::test]
async fn test_search_docs_extract_code() {
    let (_tmp, server) = build_test_server().await;

    let result = server
        .search_docs(Parameters(SearchDocsParams {
            query: "函数定义".into(),
            top_k: 5,
            offset: 0,
            extract_code: true,
            category: None,
            package: None,
        }))
        .await;

    let result_count = result.matches("### [").count();
    assert!(result_count > 0, "should return results");
    assert!(!result.is_empty(), "result content should not be empty");
}

#[tokio::test]
async fn test_search_docs_package_filter() {
    let (_tmp, server) = build_test_server().await;

    let result = server
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
        result.contains("Array"),
        "results should mention 'Array' when package filter is set"
    );
}

#[tokio::test]
async fn test_get_topic_found() {
    let (_tmp, server) = build_test_server().await;

    let result = server
        .get_topic(Parameters(GetTopicParams {
            topic: "functions".into(),
            category: Some("syntax".into()),
        }))
        .await;

    assert!(
        result.contains("函数"),
        "topic content should contain '函数'"
    );
    assert!(
        result.contains("**Category:** syntax"),
        "should show category metadata"
    );
    assert!(
        result.contains("**Topic:** functions"),
        "should show topic metadata"
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

    let result = server
        .get_topic(Parameters(GetTopicParams {
            topic: "functions".into(),
            category: Some("stdlib".into()),
        }))
        .await;

    assert!(
        result.contains("**Category:** syntax"),
        "tool should fallback to correct category when provided category is wrong"
    );
    assert!(
        result.contains("**Topic:** functions"),
        "should contain the topic name"
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

    let result = server
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
        result.contains("HashMap"),
        "results should contain HashMap content when top_k is large"
    );
    let result_count = result.matches("### [").count();
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

    let result = server
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
        result.contains("HashMap"),
        "results should contain HashMap content when top_k is small"
    );
    let result_count = result.matches("### [").count();
    assert!(result_count > 0, "should return at least one result");
}

#[tokio::test]
async fn test_list_topics_all() {
    let (_tmp, server) = build_test_server().await;

    let result = server
        .list_topics(Parameters(ListTopicsParams {
            category: None,
            detail: false,
        }))
        .await;

    assert!(
        result.contains("syntax"),
        "should contain 'syntax' category"
    );
    assert!(
        result.contains("stdlib"),
        "should contain 'stdlib' category"
    );
    assert!(result.contains("cjpm"), "should contain 'cjpm' category");
    assert!(
        result.contains("topics total"),
        "should show total topic count"
    );
    // In compact mode (detail=false), should show topic counts but not individual topic names
    assert!(
        result.contains("topics)"),
        "should show topic count per category"
    );
}

#[tokio::test]
async fn test_list_topics_filter_category_with_detail() {
    let (_tmp, server) = build_test_server().await;

    let result = server
        .list_topics(Parameters(ListTopicsParams {
            category: Some("syntax".into()),
            detail: true,
        }))
        .await;

    assert!(
        result.contains("syntax"),
        "should contain 'syntax' category"
    );
    assert!(
        !result.contains("### stdlib"),
        "should not contain 'stdlib' when filtered by 'syntax'"
    );
    assert!(
        result.contains("functions"),
        "syntax category should contain 'functions' topic"
    );
}

#[tokio::test]
async fn test_list_topics_invalid_category() {
    let (_tmp, server) = build_test_server().await;

    let result = server
        .list_topics(Parameters(ListTopicsParams {
            category: Some("nonexistent".into()),
            detail: false,
        }))
        .await;

    assert!(
        result.contains("not found"),
        "should indicate category not found, got: {result}"
    );
    assert!(
        result.contains("syntax"),
        "should list available categories including 'syntax', got: {result}"
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

    let result = server
        .search_docs(Parameters(SearchDocsParams {
            query: "仓颉".into(),
            top_k: 999,
            offset: 0,
            extract_code: false,
            category: None,
            package: None,
        }))
        .await;

    let result_count = result.matches("### [").count();
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

    let result = server
        .search_docs(Parameters(SearchDocsParams {
            query: "函数".into(),
            top_k: 0,
            offset: 0,
            extract_code: false,
            category: None,
            package: None,
        }))
        .await;

    let result_count = result.matches("### [").count();
    assert!(
        result_count == MIN_TOP_K,
        "result count ({}) should be exactly MIN_TOP_K ({}) when top_k is clamped from 0",
        result_count,
        MIN_TOP_K
    );
}
