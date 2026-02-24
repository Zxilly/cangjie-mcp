use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use anyhow::Result;
use async_trait::async_trait;

use cangjie_mcp::indexer::embedding::Embedder;
use cangjie_mcp::indexer::search::vector::VectorStore;
use cangjie_mcp::indexer::{DocMetadata, TextChunk};

// -- MockEmbedder -------------------------------------------------------------

const DIM: usize = 8;

/// A deterministic embedder that produces normalized vectors derived from a hash
/// of the input text.  No API key or model required.
struct MockEmbedder;

impl MockEmbedder {
    fn hash_to_vec(text: &str) -> Vec<f32> {
        let mut v = Vec::with_capacity(DIM);
        for i in 0..DIM {
            let mut h = DefaultHasher::new();
            (text, i).hash(&mut h);
            // Map hash → [-1, 1]
            let val = (h.finish() as f64 / u64::MAX as f64) * 2.0 - 1.0;
            v.push(val as f32);
        }
        // L2 normalize
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for x in &mut v {
                *x /= norm;
            }
        }
        v
    }
}

#[async_trait]
impl Embedder for MockEmbedder {
    async fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|t| Self::hash_to_vec(t)).collect())
    }
    fn model_name(&self) -> &str {
        "mock"
    }
}

// -- Helpers ------------------------------------------------------------------

fn make_chunk(text: &str, category: &str, topic: &str) -> TextChunk {
    TextChunk {
        text: text.to_string(),
        metadata: DocMetadata {
            file_path: format!("{category}/{topic}.md"),
            category: category.to_string(),
            topic: topic.to_string(),
            title: topic.to_string(),
            has_code: false,
            code_block_count: 0,
        },
    }
}

fn sample_chunks() -> Vec<TextChunk> {
    vec![
        make_chunk("变量声明使用 let 关键字", "syntax", "variables"),
        make_chunk("函数定义使用 func 关键字", "syntax", "functions"),
        make_chunk("类和结构体的面向对象编程", "advanced", "classes"),
        make_chunk("泛型允许编写灵活可复用的代码", "advanced", "generics"),
        make_chunk("错误处理使用 try catch", "syntax", "errors"),
    ]
}

// -- Tests --------------------------------------------------------------------

#[tokio::test]
async fn test_vector_store_open_empty() {
    let tmp = tempfile::tempdir().unwrap();
    let vs = VectorStore::open(tmp.path(), DIM).await.unwrap();
    assert!(!vs.is_ready(), "Fresh store should not be ready");
}

#[tokio::test]
async fn test_vector_store_build_and_ready() {
    let tmp = tempfile::tempdir().unwrap();
    let mut vs = VectorStore::open(tmp.path(), DIM).await.unwrap();
    let embedder = MockEmbedder;
    vs.build_from_chunks(&sample_chunks(), &embedder, 64)
        .await
        .unwrap();
    assert!(vs.is_ready(), "Store should be ready after build");
}

#[tokio::test]
async fn test_vector_store_search_basic() {
    let tmp = tempfile::tempdir().unwrap();
    let mut vs = VectorStore::open(tmp.path(), DIM).await.unwrap();
    let embedder = MockEmbedder;
    vs.build_from_chunks(&sample_chunks(), &embedder, 64)
        .await
        .unwrap();

    let query_emb = MockEmbedder::hash_to_vec("变量");
    let results = vs.search(&query_emb, 3, None).await.unwrap();
    assert!(!results.is_empty(), "Search should return results");
    // All results should have valid metadata
    for r in &results {
        assert!(!r.text.is_empty());
        assert!(!r.metadata.file_path.is_empty());
        assert!(!r.metadata.category.is_empty());
        assert!(r.score > 0.0);
    }
}

#[tokio::test]
async fn test_vector_store_search_returns_top_k() {
    let tmp = tempfile::tempdir().unwrap();
    let mut vs = VectorStore::open(tmp.path(), DIM).await.unwrap();
    let embedder = MockEmbedder;

    // Build with 11 chunks
    let mut chunks = sample_chunks();
    for i in 0..6 {
        chunks.push(make_chunk(
            &format!("Extra chunk number {i}"),
            "extra",
            &format!("extra_{i}"),
        ));
    }
    vs.build_from_chunks(&chunks, &embedder, 64).await.unwrap();

    let query_emb = MockEmbedder::hash_to_vec("test");
    let results = vs.search(&query_emb, 3, None).await.unwrap();
    assert!(
        results.len() <= 3,
        "Should return at most top_k=3 results, got {}",
        results.len()
    );
}

#[tokio::test]
async fn test_vector_store_search_category_filter() {
    let tmp = tempfile::tempdir().unwrap();
    let mut vs = VectorStore::open(tmp.path(), DIM).await.unwrap();
    let embedder = MockEmbedder;
    vs.build_from_chunks(&sample_chunks(), &embedder, 64)
        .await
        .unwrap();

    let query_emb = MockEmbedder::hash_to_vec("test");
    let results = vs.search(&query_emb, 10, Some("syntax")).await.unwrap();
    for r in &results {
        assert_eq!(
            r.metadata.category, "syntax",
            "All results should be in the 'syntax' category"
        );
    }
    assert!(
        !results.is_empty(),
        "Should find at least one 'syntax' result"
    );
}

#[tokio::test]
async fn test_vector_store_search_nonexistent_category() {
    let tmp = tempfile::tempdir().unwrap();
    let mut vs = VectorStore::open(tmp.path(), DIM).await.unwrap();
    let embedder = MockEmbedder;
    vs.build_from_chunks(&sample_chunks(), &embedder, 64)
        .await
        .unwrap();

    let query_emb = MockEmbedder::hash_to_vec("test");
    let results = vs.search(&query_emb, 5, Some("nonexistent")).await.unwrap();
    assert!(
        results.is_empty(),
        "Non-existent category should return empty results"
    );
}

#[tokio::test]
async fn test_vector_store_search_not_ready() {
    let tmp = tempfile::tempdir().unwrap();
    let vs = VectorStore::open(tmp.path(), DIM).await.unwrap();

    let query_emb = MockEmbedder::hash_to_vec("test");
    let results = vs.search(&query_emb, 5, None).await.unwrap();
    assert!(
        results.is_empty(),
        "Search on unbuilt store should return empty"
    );
}

#[tokio::test]
async fn test_vector_store_rebuild_replaces_data() {
    let tmp = tempfile::tempdir().unwrap();
    let mut vs = VectorStore::open(tmp.path(), DIM).await.unwrap();
    let embedder = MockEmbedder;

    // First build
    let chunks1 = vec![make_chunk("First version data", "v1", "old")];
    vs.build_from_chunks(&chunks1, &embedder, 64).await.unwrap();

    // Second build replaces data
    let chunks2 = vec![make_chunk("Second version data", "v2", "new")];
    vs.build_from_chunks(&chunks2, &embedder, 64).await.unwrap();

    let query_emb = MockEmbedder::hash_to_vec("data");
    let results = vs.search(&query_emb, 10, None).await.unwrap();

    // All results should be from v2
    for r in &results {
        assert_eq!(
            r.metadata.category, "v2",
            "After rebuild, only new data should exist"
        );
    }
}

#[tokio::test]
async fn test_vector_store_reopen_persists() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().to_path_buf();

    {
        let mut vs = VectorStore::open(&path, DIM).await.unwrap();
        let embedder = MockEmbedder;
        vs.build_from_chunks(&sample_chunks(), &embedder, 64)
            .await
            .unwrap();
        assert!(vs.is_ready());
    }

    // Reopen
    let vs2 = VectorStore::open(&path, DIM).await.unwrap();
    assert!(vs2.is_ready(), "Reopened store should still be ready");
}

#[tokio::test]
async fn test_vector_store_score_ordering() {
    let tmp = tempfile::tempdir().unwrap();
    let mut vs = VectorStore::open(tmp.path(), DIM).await.unwrap();
    let embedder = MockEmbedder;
    vs.build_from_chunks(&sample_chunks(), &embedder, 64)
        .await
        .unwrap();

    let query_emb = MockEmbedder::hash_to_vec("变量");
    let results = vs.search(&query_emb, 5, None).await.unwrap();

    for w in results.windows(2) {
        assert!(
            w[0].score >= w[1].score,
            "Results should be ordered by descending score: {} >= {}",
            w[0].score,
            w[1].score
        );
    }
}

#[tokio::test]
async fn test_vector_store_empty_chunks_error() {
    let tmp = tempfile::tempdir().unwrap();
    let mut vs = VectorStore::open(tmp.path(), DIM).await.unwrap();
    let embedder = MockEmbedder;
    let result = vs.build_from_chunks(&[], &embedder, 64).await;
    assert!(result.is_err(), "Building from empty chunks should error");
}
