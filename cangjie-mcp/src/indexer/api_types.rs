//! OpenAI-compatible API response types shared across embedding, reranking, and chat modules.

use serde::Deserialize;

// -- Embeddings API ---

#[derive(Debug, Deserialize)]
pub struct EmbeddingsResponse {
    pub data: Vec<EmbeddingData>,
}

#[derive(Debug, Deserialize)]
pub struct EmbeddingData {
    pub embedding: Vec<f32>,
}

// -- Rerank API ---

#[derive(Debug, Deserialize)]
pub struct RerankResponse {
    pub results: Vec<RerankItem>,
}

#[derive(Debug, Deserialize)]
pub struct RerankItem {
    pub index: usize,
    pub relevance_score: f64,
}

// -- Chat Completions API ---

#[derive(Debug, Deserialize)]
pub struct ChatResponse {
    pub choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
pub struct ChatChoice {
    pub message: ChatMessage,
}

#[derive(Debug, Deserialize)]
pub struct ChatMessage {
    pub content: String,
}
