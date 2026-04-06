use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Json;
use axum::routing::{get, post};
use axum::Router;
use serde::{Deserialize, Serialize};

use cangjie_indexer::search::LocalSearchIndex;
use cangjie_indexer::IndexMetadata;

// ── Shared state ────────────────────────────────────────────────────────────

struct AppState {
    search_index: Arc<LocalSearchIndex>,
    index_metadata: IndexMetadata,
}

// ── Request/Response types ──────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct SearchRequest {
    query: String,
    #[serde(default = "default_top_k")]
    top_k: usize,
    category: Option<String>,
}

fn default_top_k() -> usize {
    cangjie_core::config::DEFAULT_TOP_K
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
    search_mode: cangjie_indexer::SearchMode,
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
        search_mode: state.index_metadata.search_mode,
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
        .query(&req.query, req.top_k, category)
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

// ── App builder ─────────────────────────────────────────────────────────────

pub async fn create_http_app(
    search_index: Arc<LocalSearchIndex>,
    index_metadata: IndexMetadata,
) -> Router {
    let state = Arc::new(AppState {
        search_index,
        index_metadata,
    });

    Router::new()
        .route("/health", get(health))
        .route("/info", get(info_handler))
        .route("/search", post(search_handler))
        .with_state(state)
}
