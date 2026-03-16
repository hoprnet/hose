use axum::Router;
use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::sync::broadcast;

use crate::config::Config;
use crate::peer_tracker::PeerTracker;

/// Shared application state accessible from all HTTP handlers.
#[derive(Debug, Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub db: SqlitePool,
    pub peer_tracker: PeerTracker,
    /// Broadcast channel for SSE live events.
    pub event_tx: broadcast::Sender<Event>,
}

/// Events pushed to connected browsers via SSE.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    PeerSeen { peer_id: String },
    SessionObserved { session_id: String },
    DebugSessionUpdated { session_id: String },
    TelemetryRate { records_per_second: f64 },
}

impl AppState {
    pub fn new(config: Config, db: SqlitePool, peer_tracker: PeerTracker) -> Self {
        let (event_tx, _) = broadcast::channel(1024);
        Self {
            config: Arc::new(config),
            db,
            peer_tracker,
            event_tx,
        }
    }

    /// Emit an event to all connected SSE clients. Drops silently if no receivers.
    pub fn emit(&self, event: Event) {
        let _ = self.event_tx.send(event);
    }
}

/// Build the Axum router with all routes and middleware.
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", axum::routing::get(health_check))
        .route("/api/peers", axum::routing::get(crate::api::peers::list_peers))
        .with_state(state)
        .layer(tower_http::trace::TraceLayer::new_for_http())
}

/// Health check endpoint.
async fn health_check() -> &'static str {
    "ok"
}

/// Start the HTTP server on the configured address.
pub async fn run(state: AppState) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let addr = state.config.http_listen_addr;
    let router = build_router(state);

    tracing::info!(%addr, "HTTP server listening");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, router).await?;

    Ok(())
}
