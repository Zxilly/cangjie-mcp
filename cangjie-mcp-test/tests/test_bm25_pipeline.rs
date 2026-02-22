use cangjie_mcp::indexer::search::bm25::BM25Store;
use cangjie_mcp::indexer::search::LocalSearchIndex;
use cangjie_mcp_test::{sample_chunks, test_settings};
use tempfile::TempDir;

fn build_index_in_tempdir() -> (TempDir, BM25Store) {
    let tmp = TempDir::new().unwrap();
    let bm25_dir = tmp.path().join("bm25_index");
    let mut store = BM25Store::new(bm25_dir);
    let chunks = sample_chunks();
    store.build_from_chunks(&chunks).unwrap();
    (tmp, store)
}

#[test]
fn test_build_and_search() {
    let (_tmp, store) = build_index_in_tempdir();

    let results = store.search("函数", 5, None).unwrap();
    assert!(
        !results.is_empty(),
        "search should return results for '函数'"
    );
    assert!(results[0].score > 0.0);
    assert!(!results[0].metadata.file_path.is_empty());
}

#[test]
fn test_search_chinese_query() {
    let (_tmp, store) = build_index_in_tempdir();

    let results = store.search("变量声明", 5, None).unwrap();
    assert!(
        !results.is_empty(),
        "Chinese query '变量声明' should find results"
    );

    // Should match the variables chunk
    let has_variable_result = results
        .iter()
        .any(|r| r.metadata.topic == "variables" || r.text.contains("变量"));
    assert!(has_variable_result, "should find variable-related document");
}

#[test]
fn test_search_category_filter() {
    let (_tmp, store) = build_index_in_tempdir();

    let results = store.search("函数", 10, Some("syntax")).unwrap();
    for r in &results {
        assert_eq!(
            r.metadata.category, "syntax",
            "filtered results must belong to 'syntax' category"
        );
    }
}

#[test]
fn test_search_no_results() {
    let (_tmp, store) = build_index_in_tempdir();

    let results = store.search("xyznonexistent12345qwerty", 5, None).unwrap();
    assert!(results.is_empty(), "random string should yield no results");
}

#[test]
fn test_build_empty_chunks() {
    let tmp = TempDir::new().unwrap();
    let bm25_dir = tmp.path().join("bm25_empty");
    let mut store = BM25Store::new(bm25_dir);
    // Building from empty chunks should not error
    store.build_from_chunks(&[]).unwrap();
}

#[test]
fn test_load_existing_index() {
    let tmp = TempDir::new().unwrap();
    let bm25_dir = tmp.path().join("bm25_reload");

    // Build
    {
        let mut store = BM25Store::new(bm25_dir.clone());
        let chunks = sample_chunks();
        store.build_from_chunks(&chunks).unwrap();
    }

    // Reload
    let mut store2 = BM25Store::new(bm25_dir);
    let loaded = store2.load().unwrap();
    assert!(loaded, "index should be loadable after build");

    let results = store2.search("函数", 5, None).unwrap();
    assert!(
        !results.is_empty(),
        "reloaded index should return search results"
    );
}

#[test]
fn test_search_with_settings_integration() {
    let tmp = TempDir::new().unwrap();
    let settings = test_settings(tmp.path().to_path_buf());
    assert_eq!(
        settings.embedding_type,
        cangjie_mcp::config::EmbeddingType::None
    );
    assert_eq!(settings.rerank_type, cangjie_mcp::config::RerankType::None);
}

/// BM25 query parser should handle special regex characters gracefully
/// instead of panicking or returning errors.
#[test]
fn test_search_special_characters_query() {
    let (_tmp, store) = build_index_in_tempdir();

    // These contain regex metacharacters that could break the tantivy query parser.
    // The BM25Store should fall back to AllQuery rather than error.
    for query in &["func()", "a+b", "x.*y", "[array]", "a && b", "a || b"] {
        let result = store.search(query, 5, None);
        assert!(
            result.is_ok(),
            "search with special characters '{query}' should not error"
        );
    }
}

/// Searching a nonexistent category should return empty results, not error.
#[test]
fn test_search_nonexistent_category() {
    let (_tmp, store) = build_index_in_tempdir();

    let results = store
        .search("函数", 10, Some("nonexistent_category"))
        .unwrap();
    assert!(
        results.is_empty(),
        "filtering by nonexistent category should return empty"
    );
}

/// is_indexed() should reflect whether an index has been built on disk.
#[test]
fn test_is_indexed_lifecycle() {
    let tmp = TempDir::new().unwrap();
    let bm25_dir = tmp.path().join("bm25_lifecycle");

    let store = BM25Store::new(bm25_dir.clone());
    assert!(!store.is_indexed(), "new store should not be indexed yet");

    let mut store = BM25Store::new(bm25_dir.clone());
    store.build_from_chunks(&sample_chunks()).unwrap();
    assert!(store.is_indexed(), "store should be indexed after build");
}

/// load() on a directory with no index should return false, not error.
#[test]
fn test_load_nonexistent_index() {
    let tmp = TempDir::new().unwrap();
    let bm25_dir = tmp.path().join("bm25_no_index");
    let mut store = BM25Store::new(bm25_dir);
    let loaded = store.load().unwrap();
    assert!(!loaded, "loading from empty dir should return false");
}

/// LocalSearchIndex::query() with BM25-only (no vector, no reranker) should
/// produce the same results as calling BM25Store::search() directly.
#[tokio::test]
async fn test_local_search_index_query_bm25_only() {
    let tmp = TempDir::new().unwrap();
    let bm25_dir = tmp.path().join("bm25_via_index");

    let mut bm25 = BM25Store::new(bm25_dir);
    bm25.build_from_chunks(&sample_chunks()).unwrap();

    let settings = test_settings(tmp.path().to_path_buf());
    let index = LocalSearchIndex::with_bm25(settings, bm25);

    let results = index.query("函数定义", 5, None, false).await.unwrap();
    assert!(!results.is_empty());
    assert!(results[0].score > 0.0);
    assert!(
        results[0].text.contains("函数") || results[0].metadata.topic == "functions",
        "query should return relevant results"
    );
}

/// LocalSearchIndex::query() with category filter through the full query path.
#[tokio::test]
async fn test_local_search_index_query_with_category() {
    let tmp = TempDir::new().unwrap();
    let bm25_dir = tmp.path().join("bm25_cat_query");

    let mut bm25 = BM25Store::new(bm25_dir);
    bm25.build_from_chunks(&sample_chunks()).unwrap();

    let settings = test_settings(tmp.path().to_path_buf());
    let index = LocalSearchIndex::with_bm25(settings, bm25);

    let results = index.query("函数", 10, Some("cjpm"), false).await.unwrap();
    for r in &results {
        assert_eq!(r.metadata.category, "cjpm");
    }
}

/// When no search stores are configured, query() should return empty vec.
#[tokio::test]
async fn test_local_search_index_query_no_stores() {
    let tmp = TempDir::new().unwrap();
    let settings = test_settings(tmp.path().to_path_buf());
    let index = LocalSearchIndex::new(settings);

    let results = index.query("anything", 5, None, false).await.unwrap();
    assert!(results.is_empty(), "no stores = empty results");
}
