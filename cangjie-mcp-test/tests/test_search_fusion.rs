use cangjie_mcp::indexer::search::bm25::BM25Store;
use cangjie_mcp::indexer::search::fusion::reciprocal_rank_fusion;
use cangjie_mcp::indexer::{SearchResult, SearchResultMetadata};
use cangjie_mcp_test::sample_chunks;
use tempfile::TempDir;

fn make_result(text: &str, score: f64, file: &str, category: &str) -> SearchResult {
    SearchResult {
        text: text.to_string(),
        score,
        metadata: SearchResultMetadata {
            file_path: file.to_string(),
            category: category.to_string(),
            topic: "test".to_string(),
            title: "Test".to_string(),
            has_code: false,
        },
    }
}

#[tokio::test]
async fn test_bm25_search_and_fusion() {
    let tmp = TempDir::new().unwrap();
    let bm25_dir = tmp.path().join("bm25_fusion");
    let mut store = BM25Store::new(bm25_dir);
    store.build_from_chunks(&sample_chunks()).await.unwrap();

    // Two different queries
    let results_a = store.search("函数定义", 5, None).await.unwrap();
    let results_b = store.search("集合类型", 5, None).await.unwrap();

    assert!(!results_a.is_empty());
    assert!(!results_b.is_empty());

    // Fuse them
    let fused = reciprocal_rank_fusion(&[results_a, results_b], 60, 10);
    assert!(!fused.is_empty(), "fused results should not be empty");

    // Scores should be positive and ordered
    for window in fused.windows(2) {
        assert!(
            window[0].score >= window[1].score,
            "fused results should be in descending score order"
        );
    }
}

#[test]
fn test_fusion_dedup() {
    // Two lists with overlapping results
    let list1 = vec![
        make_result(
            "shared document about functions",
            0.9,
            "shared.md",
            "syntax",
        ),
        make_result("only in list 1", 0.7, "list1.md", "syntax"),
    ];
    let list2 = vec![
        make_result(
            "shared document about functions",
            0.8,
            "shared.md",
            "syntax",
        ),
        make_result("only in list 2", 0.6, "list2.md", "stdlib"),
    ];

    let fused = reciprocal_rank_fusion(&[list1, list2], 60, 10);

    // "shared document about functions" should appear only once
    let shared_count = fused
        .iter()
        .filter(|r| r.metadata.file_path == "shared.md")
        .count();
    assert_eq!(shared_count, 1, "duplicate results should be merged");

    // The shared result should have the highest score (appears in both lists)
    assert_eq!(
        fused[0].metadata.file_path, "shared.md",
        "shared result should be ranked first due to score accumulation"
    );

    // Total unique results should be 3
    assert_eq!(fused.len(), 3);
}

#[test]
fn test_fusion_score_accumulation() {
    let list1 = vec![
        make_result("doc A", 1.0, "a.md", "cat"),
        make_result("doc B", 0.8, "b.md", "cat"),
    ];
    let list2 = vec![
        make_result("doc B", 0.9, "b.md", "cat"),
        make_result("doc C", 0.7, "c.md", "cat"),
    ];

    let fused = reciprocal_rank_fusion(&[list1, list2], 60, 10);

    // doc B appears in both lists so should have accumulated score
    let doc_b = fused
        .iter()
        .find(|r| r.metadata.file_path == "b.md")
        .unwrap();
    let doc_a = fused
        .iter()
        .find(|r| r.metadata.file_path == "a.md")
        .unwrap();
    let doc_c = fused
        .iter()
        .find(|r| r.metadata.file_path == "c.md")
        .unwrap();

    // doc B (rank 1 in list1 + rank 0 in list2) should accumulate more RRF score
    // than doc C (only in list2 at rank 1)
    assert!(
        doc_b.score > doc_c.score,
        "doc appearing in both lists should have higher score"
    );

    // doc A (rank 0 in list1 only) gets 1/(60+1) = 0.01639
    // doc B (rank 1 in list1 + rank 0 in list2) gets 1/(60+2) + 1/(60+1) = 0.01613 + 0.01639 = 0.03252
    assert!(
        doc_b.score > doc_a.score,
        "doc in both lists should outrank doc in single list"
    );
}

/// Three-way fusion should work correctly, combining results from 3 sources.
#[test]
fn test_fusion_three_lists() {
    let list1 = vec![
        make_result("doc A", 1.0, "a.md", "cat"),
        make_result("doc B", 0.8, "b.md", "cat"),
    ];
    let list2 = vec![
        make_result("doc B", 0.9, "b.md", "cat"),
        make_result("doc C", 0.7, "c.md", "cat"),
    ];
    let list3 = vec![
        make_result("doc B", 0.95, "b.md", "cat"),
        make_result("doc D", 0.6, "d.md", "cat"),
    ];

    let fused = reciprocal_rank_fusion(&[list1, list2, list3], 60, 10);

    // doc B appears in all 3 lists, should be ranked first
    assert_eq!(fused[0].metadata.file_path, "b.md");
    // Should have 4 unique results total
    assert_eq!(fused.len(), 4);
}

/// top_k should actually limit the output size.
#[test]
fn test_fusion_top_k_limits_output() {
    let list1 = vec![
        make_result("doc A", 1.0, "a.md", "cat"),
        make_result("doc B", 0.9, "b.md", "cat"),
        make_result("doc C", 0.8, "c.md", "cat"),
        make_result("doc D", 0.7, "d.md", "cat"),
    ];

    let fused = reciprocal_rank_fusion(&[list1], 60, 2);
    assert_eq!(fused.len(), 2, "output should be limited to top_k=2");
}

/// Single empty list in fusion should produce empty output.
#[test]
fn test_fusion_single_empty_list() {
    let fused = reciprocal_rank_fusion(&[vec![]], 60, 5);
    assert!(fused.is_empty());
}

/// BM25 search for English keywords should also work (not just Chinese).
#[tokio::test]
async fn test_bm25_english_search() {
    let tmp = TempDir::new().unwrap();
    let bm25_dir = tmp.path().join("bm25_english");
    let mut store = BM25Store::new(bm25_dir);
    store.build_from_chunks(&sample_chunks()).await.unwrap();

    // sample_chunks contains "Array", "HashMap", "CJPM", "Result" etc.
    let results = store.search("Array HashMap", 5, None).await.unwrap();
    assert!(
        !results.is_empty(),
        "English keywords should find results in mixed content"
    );
}

/// BM25 search with top_k=1 should return exactly one result.
#[tokio::test]
async fn test_bm25_top_k_one() {
    let tmp = TempDir::new().unwrap();
    let bm25_dir = tmp.path().join("bm25_topk1");
    let mut store = BM25Store::new(bm25_dir);
    store.build_from_chunks(&sample_chunks()).await.unwrap();

    let results = store.search("函数", 1, None).await.unwrap();
    assert_eq!(results.len(), 1, "top_k=1 should return exactly 1 result");
}
