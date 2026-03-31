use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use futures::channel::mpsc as futures_mpsc;
use futures::{SinkExt, StreamExt};
use rmcp::model::{ClientJsonRpcMessage, ServerJsonRpcMessage};
use rmcp::serve_server;
use serde::Deserialize;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

use crate::CangjieServer;

static SESSION_COUNTER: AtomicU64 = AtomicU64::new(0);

fn new_session_id() -> String {
    let count = SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    format!("{ts:x}-{count:x}")
}

type SessionSender = futures_mpsc::Sender<ClientJsonRpcMessage>;
type SessionMap = Arc<RwLock<HashMap<String, SessionSender>>>;

#[derive(Clone)]
struct SseState {
    sessions: SessionMap,
    server_factory: Arc<dyn Fn() -> CangjieServer + Send + Sync>,
}

#[derive(Debug, Deserialize)]
struct MessageQuery {
    #[serde(alias = "sessionId")]
    session_id: String,
}

async fn sse_handler(State(state): State<SseState>) -> impl IntoResponse {
    let session_id = new_session_id();

    // Channels for the rmcp transport (server-side perspective):
    //   client_tx/client_rx: POST handler → transport (client messages)
    //   server_tx/server_rx: transport → SSE stream  (server messages)
    let (client_tx, client_rx) = futures_mpsc::channel::<ClientJsonRpcMessage>(32);
    let (server_tx, mut server_rx) = futures_mpsc::channel::<ServerJsonRpcMessage>(32);

    state
        .sessions
        .write()
        .await
        .insert(session_id.clone(), client_tx);

    // Spawn the MCP server with a (Sink, Stream) transport
    let server = (state.server_factory)();
    let sid = session_id.clone();
    tokio::spawn(async move {
        match serve_server(server, (server_tx, client_rx)).await {
            Ok(running) => {
                info!("SSE session {sid} initialized");
                running.waiting().await.ok();
            }
            Err(e) => {
                error!("SSE session {sid} init failed: {e}");
            }
        }
    });

    // Build SSE event stream via a futures channel (implements Stream directly)
    let (mut event_tx, event_rx) = futures_mpsc::channel::<Result<Event, Infallible>>(32);
    let sessions = state.sessions.clone();
    let sid = session_id.clone();

    tokio::spawn(async move {
        // 1) Send the `endpoint` event so the client knows where to POST
        let endpoint = format!("/sse?sessionId={sid}");
        if event_tx
            .send(Ok(Event::default().event("endpoint").data(endpoint)))
            .await
            .is_err()
        {
            sessions.write().await.remove(&sid);
            return;
        }

        // 2) Forward server JSON-RPC messages as SSE `message` events
        while let Some(msg) = server_rx.next().await {
            match serde_json::to_string(&msg) {
                Ok(json) => {
                    if event_tx
                        .send(Ok(Event::default().event("message").data(json)))
                        .await
                        .is_err()
                    {
                        break; // client disconnected
                    }
                }
                Err(e) => warn!("SSE serialize error: {e}"),
            }
        }

        sessions.write().await.remove(&sid);
    });

    Sse::new(event_rx).keep_alive(
        KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
            .text("ping"),
    )
}

async fn message_handler(
    State(state): State<SseState>,
    Query(query): Query<MessageQuery>,
    body: String,
) -> StatusCode {
    let sessions = state.sessions.read().await;
    let Some(mut sender) = sessions.get(&query.session_id).cloned() else {
        return StatusCode::NOT_FOUND;
    };
    drop(sessions);

    let Ok(message) = serde_json::from_str::<ClientJsonRpcMessage>(&body) else {
        return StatusCode::BAD_REQUEST;
    };

    if sender.send(message).await.is_err() {
        return StatusCode::GONE;
    }

    StatusCode::ACCEPTED
}

/// Create an axum Router implementing the legacy MCP SSE transport.
///
/// Endpoint:
/// - `GET  /sse` — opens an SSE stream; sends an `endpoint` event with the POST URL
/// - `POST /sse?sessionId=<id>` — receives JSON-RPC messages from the client
pub fn create_sse_router(
    server_factory: impl Fn() -> CangjieServer + Send + Sync + 'static,
) -> Router {
    let state = SseState {
        sessions: Arc::new(RwLock::new(HashMap::new())),
        server_factory: Arc::new(server_factory),
    };

    Router::new()
        .route("/sse", get(sse_handler).post(message_handler))
        .with_state(state)
}
