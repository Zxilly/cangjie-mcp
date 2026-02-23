use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::Json;
use axum::routing::{get, post};
use axum::Router;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::indexer::document::source::DocumentSource;
use crate::indexer::search::LocalSearchIndex;
use crate::indexer::IndexMetadata;

// ── Shared state ────────────────────────────────────────────────────────────

struct AppState {
    search_index: LocalSearchIndex,
    doc_source: Box<dyn DocumentSource>,
    index_metadata: IndexMetadata,
    topics_cache: HashMap<String, Vec<TopicEntry>>,
}

#[derive(Debug, Clone, Serialize)]
struct TopicEntry {
    name: String,
    title: String,
}

// ── Request/Response types ──────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct SearchRequest {
    query: String,
    #[serde(default = "default_top_k")]
    top_k: usize,
    category: Option<String>,
    #[serde(default = "default_true")]
    rerank: bool,
}

fn default_top_k() -> usize {
    crate::config::DEFAULT_TOP_K
}
fn default_true() -> bool {
    true
}

#[derive(Debug, Serialize)]
struct SearchResponse {
    results: Vec<SearchResultResponse>,
}

#[derive(Debug, Serialize)]
struct SearchResultResponse {
    text: String,
    score: f64,
    metadata: MetadataResponse,
}

#[derive(Debug, Serialize)]
struct MetadataResponse {
    file_path: String,
    category: String,
    topic: String,
    title: String,
    has_code: bool,
}

#[derive(Debug, Serialize)]
struct InfoResponse {
    version: String,
    lang: String,
    embedding_model: String,
    document_count: usize,
    search_mode: String,
}

#[derive(Debug, Serialize)]
struct TopicResponse {
    content: String,
    file_path: String,
    category: String,
    topic: String,
    title: String,
}

#[derive(Debug, Serialize)]
struct TopicsResponse {
    categories: HashMap<String, Vec<TopicEntry>>,
}

// ── Route handlers ──────────────────────────────────────────────────────────

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({"status": "ok"}))
}

async fn info_handler(State(state): State<Arc<AppState>>) -> Json<InfoResponse> {
    Json(InfoResponse {
        version: state.index_metadata.version.clone(),
        lang: state.index_metadata.lang.clone(),
        embedding_model: state.index_metadata.embedding_model.clone(),
        document_count: state.index_metadata.document_count,
        search_mode: state.index_metadata.search_mode.clone(),
    })
}

async fn search_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SearchRequest>,
) -> Result<Json<SearchResponse>, StatusCode> {
    if req.query.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let category = req.category.as_deref();
    let results = state
        .search_index
        .query(&req.query, req.top_k, category, req.rerank)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let response = SearchResponse {
        results: results
            .into_iter()
            .map(|r| SearchResultResponse {
                text: r.text,
                score: r.score,
                metadata: MetadataResponse {
                    file_path: r.metadata.file_path,
                    category: r.metadata.category,
                    topic: r.metadata.topic,
                    title: r.metadata.title,
                    has_code: r.metadata.has_code,
                },
            })
            .collect(),
    };

    Ok(Json(response))
}

async fn topics_handler(State(state): State<Arc<AppState>>) -> Json<TopicsResponse> {
    Json(TopicsResponse {
        categories: state.topics_cache.clone(),
    })
}

async fn topic_detail_handler(
    State(state): State<Arc<AppState>>,
    Path((category, topic)): Path<(String, String)>,
) -> (StatusCode, Json<serde_json::Value>) {
    let doc = match state
        .doc_source
        .get_document_by_topic(&topic, Some(&category))
        .await
    {
        Ok(Some(doc)) => doc,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(
                    serde_json::json!({"error": format!("Topic '{topic}' not found in category '{category}'")}),
                ),
            );
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Internal server error"})),
            );
        }
    };

    let response = TopicResponse {
        content: doc.text,
        file_path: doc.metadata.file_path,
        category: doc.metadata.category,
        topic: doc.metadata.topic,
        title: doc.metadata.title,
    };
    match serde_json::to_value(response) {
        Ok(val) => (StatusCode::OK, Json(val)),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Serialization error"})),
        ),
    }
}

// ── App builder ─────────────────────────────────────────────────────────────

pub async fn create_http_app(
    search_index: LocalSearchIndex,
    doc_source: Box<dyn DocumentSource>,
    index_metadata: IndexMetadata,
) -> Router {
    // Pre-compute topics cache
    let mut topics_cache = HashMap::new();
    if let Ok(categories) = doc_source.get_categories().await {
        for cat in &categories {
            if let Ok(topics) = doc_source.get_topics_in_category(cat).await {
                let titles = doc_source.get_topic_titles(cat).await.unwrap_or_default();
                let entries: Vec<TopicEntry> = topics
                    .iter()
                    .map(|t| TopicEntry {
                        name: t.clone(),
                        title: titles.get(t).cloned().unwrap_or_default(),
                    })
                    .collect();
                topics_cache.insert(cat.clone(), entries);
            }
        }
    }
    let total_topics: usize = topics_cache.values().map(|v| v.len()).sum();
    info!(
        "Topics cache built: {} categories, {} topics",
        topics_cache.len(),
        total_topics
    );

    let state = Arc::new(AppState {
        search_index,
        doc_source,
        index_metadata,
        topics_cache,
    });

    Router::new()
        .route("/health", get(health))
        .route("/info", get(info_handler))
        .route("/search", post(search_handler))
        .route("/topics", get(topics_handler))
        .route("/topics/{category}/{topic}", get(topic_detail_handler))
        .with_state(state)
}
