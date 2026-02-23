//! Integration tests for OpenAI (SiliconFlow) embedding and reranking APIs.
//!
//! These tests require:
//! - `OPENAI_API_KEY` environment variable set (SiliconFlow key)
//! - `OPENAI_BASE_URL` environment variable (defaults to https://api.siliconflow.cn/v1)
//! - Network access
//!
//! Run with: `cargo test -p cangjie-mcp-test --test test_openai_embedding -- --ignored`
//!
//! Tip: load `.env` before running:
//!   `set -a && source .env && set +a && cargo test ...`

use cangjie_mcp::config::{DocLang, EmbeddingType, RerankType, Settings};
use cangjie_mcp::indexer::embedding;
use cangjie_mcp::indexer::rerank;
use cangjie_mcp::indexer::{SearchResult, SearchResultMetadata};

fn openai_settings() -> Option<Settings> {
    let api_key = std::env::var("OPENAI_API_KEY").ok()?;
    if api_key.is_empty() || api_key.starts_with("your-") {
        return None;
    }
    let base_url = std::env::var("OPENAI_BASE_URL")
        .unwrap_or_else(|_| "https://api.siliconflow.cn/v1".to_string());
    let model =
        std::env::var("OPENAI_EMBEDDING_MODEL").unwrap_or_else(|_| "BAAI/bge-m3".to_string());
    let rerank_model = std::env::var("CANGJIE_RERANK_MODEL")
        .unwrap_or_else(|_| "BAAI/bge-reranker-v2-m3".to_string());

    Some(Settings {
        docs_version: "test".to_string(),
        docs_lang: DocLang::Zh,
        embedding_type: EmbeddingType::OpenAI,
        local_model: String::new(),
        rerank_type: RerankType::OpenAI,
        rerank_model,
        rerank_top_k: 3,
        rerank_initial_k: 10,
        rrf_k: 60,
        chunk_max_size: 6000,
        data_dir: std::path::PathBuf::from("/tmp/test"),
        server_url: None,
        openai_api_key: Some(api_key),
        openai_base_url: base_url,
        openai_model: model,
        prebuilt: cangjie_mcp::config::PrebuiltMode::Off,
    })
}

fn make_result(text: &str, score: f64) -> SearchResult {
    SearchResult {
        text: text.to_string(),
        score,
        metadata: SearchResultMetadata {
            file_path: "test.md".to_string(),
            category: "test".to_string(),
            topic: "test".to_string(),
            title: "Test".to_string(),
            has_code: false,
        },
    }
}

#[tokio::test]
#[ignore]
async fn test_openai_embedder_creates_successfully() {
    let settings = openai_settings().expect("OPENAI_API_KEY not set");
    let embedder = embedding::create_embedder(&settings).await.unwrap();
    assert!(embedder.is_some(), "should create OpenAI embedder");
}

#[tokio::test]
#[ignore]
async fn test_openai_embed_single_text() {
    let settings = openai_settings().expect("OPENAI_API_KEY not set");
    let embedder = embedding::create_embedder(&settings)
        .await
        .unwrap()
        .unwrap();

    let texts = &["仓颉编程语言函数定义"];
    let embeddings = embedder.embed(texts).await.unwrap();

    assert_eq!(embeddings.len(), 1);
    assert!(
        !embeddings[0].is_empty(),
        "embedding vector should not be empty"
    );
    // BGE-M3 produces 1024-dim vectors
    assert!(
        embeddings[0].len() >= 256,
        "embedding dimension should be reasonable, got {}",
        embeddings[0].len()
    );
}

#[tokio::test]
#[ignore]
async fn test_openai_embed_multiple_texts() {
    let settings = openai_settings().expect("OPENAI_API_KEY not set");
    let embedder = embedding::create_embedder(&settings)
        .await
        .unwrap()
        .unwrap();

    let texts = &["函数定义", "变量声明", "错误处理"];
    let embeddings = embedder.embed(texts).await.unwrap();

    assert_eq!(embeddings.len(), 3);
    // All should have the same dimension
    let dim = embeddings[0].len();
    for (i, emb) in embeddings.iter().enumerate() {
        assert_eq!(emb.len(), dim, "embedding {} has different dimension", i);
    }
}

#[tokio::test]
#[ignore]
async fn test_openai_embed_chinese_and_english() {
    let settings = openai_settings().expect("OPENAI_API_KEY not set");
    let embedder = embedding::create_embedder(&settings)
        .await
        .unwrap()
        .unwrap();

    let texts = &["仓颉语言", "Cangjie programming language"];
    let embeddings = embedder.embed(texts).await.unwrap();
    assert_eq!(embeddings.len(), 2);

    // Compute cosine similarity — semantically related texts should have high similarity
    let dot: f32 = embeddings[0]
        .iter()
        .zip(embeddings[1].iter())
        .map(|(a, b)| a * b)
        .sum();
    let norm0: f32 = embeddings[0].iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm1: f32 = embeddings[1].iter().map(|x| x * x).sum::<f32>().sqrt();
    let cosine_sim = dot / (norm0 * norm1);

    assert!(
        cosine_sim > 0.5,
        "Chinese and English translations should be semantically similar, got cosine={}",
        cosine_sim
    );
}

#[tokio::test]
#[ignore]
async fn test_openai_embedder_model_name() {
    let settings = openai_settings().expect("OPENAI_API_KEY not set");
    let embedder = embedding::create_embedder(&settings)
        .await
        .unwrap()
        .unwrap();
    assert!(
        !embedder.model_name().is_empty(),
        "model name should not be empty"
    );
}

#[tokio::test]
#[ignore]
async fn test_openai_reranker() {
    let settings = openai_settings().expect("OPENAI_API_KEY not set");
    let reranker = rerank::create_reranker(&settings).await.unwrap();
    assert!(reranker.is_enabled(), "OpenAI reranker should be enabled");

    let results = vec![
        make_result("仓颉语言的函数定义使用 func 关键字", 0.5),
        make_result("CJPM 是仓颉的包管理器", 0.4),
        make_result("变量使用 let 和 var 声明", 0.3),
    ];

    let reranked = reranker.rerank("如何定义函数", results, 2).await.unwrap();

    assert_eq!(reranked.len(), 2, "should return top_k=2 results");
    // The function-related doc should rank higher than package manager doc
    assert!(
        reranked[0].text.contains("函数"),
        "top result should be about functions, got: {}",
        reranked[0].text
    );
}

#[tokio::test]
#[ignore]
async fn test_openai_reranker_empty_input() {
    let settings = openai_settings().expect("OPENAI_API_KEY not set");
    let reranker = rerank::create_reranker(&settings).await.unwrap();

    let reranked = reranker.rerank("query", vec![], 5).await.unwrap();
    assert!(reranked.is_empty());
}
