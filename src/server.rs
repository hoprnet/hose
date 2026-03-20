use std::sync::Arc;

use axum::{Router, routing};
use sqlx::SqlitePool;
use tokio::{net::TcpListener, sync::broadcast};
use tower_http::{services::ServeDir, trace::TraceLayer};

use crate::{
    blokli::BlokliClient, config::Config, identity::IdentityBridge, peer_router::PeerRouter, peer_tracker::PeerTracker,
    session_tracker::SessionTracker,
};

/// Shared application state accessible from all HTTP handlers.
#[derive(Debug, Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub db: SqlitePool,
    pub peer_router: PeerRouter,
    pub peer_tracker: PeerTracker,
    pub session_tracker: SessionTracker,
    pub identity_bridge: IdentityBridge,
    pub blokli_client: Option<BlokliClient>,
    /// Broadcast channel for SSE live events.
    pub event_tx: broadcast::Sender<Event>,
}

/// Events pushed to connected browsers via SSE.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    PeerSeen {
        peer_id: String,
    },
    SessionObserved {
        session_id: String,
    },
    DebugSessionUpdated {
        session_id: String,
    },
    TelemetryRate {
        records_per_second: f64,
    },
    /// A sampled trace snapshot for the live trace inspector.
    TraceSampled {
        timestamp: String,
        peer_id: String,
        span_name: String,
        trace_id: String,
        span_id: String,
        routing_decision: String,
        attributes: serde_json::Value,
    },
}

impl AppState {
    pub fn new(
        config: Config,
        db: SqlitePool,
        peer_router: PeerRouter,
        peer_tracker: PeerTracker,
        session_tracker: SessionTracker,
        identity_bridge: IdentityBridge,
        blokli_client: Option<BlokliClient>,
    ) -> Self {
        let (event_tx, _) = broadcast::channel(1024);
        Self {
            config: Arc::new(config),
            db,
            peer_router,
            peer_tracker,
            session_tracker,
            identity_bridge,
            blokli_client,
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
        // Server-rendered HTML pages
        .route("/", routing::get(crate::pages::dashboard))
        .route("/peers", routing::get(crate::pages::peers))
        .route("/sessions", routing::get(crate::pages::sessions))
        .route("/debug-sessions", routing::get(crate::pages::debug_sessions))
        .route("/debug-sessions/{id}", routing::get(crate::pages::debug_session_detail))
        .route("/inspector", routing::get(crate::pages::trace_inspector))
        // JSON API routes
        .route("/health", routing::get(health_check))
        .route("/api/peers", routing::get(crate::api::peers::list_peers))
        .route("/api/sessions", routing::get(crate::api::sessions::list_sessions))
        .route(
            "/api/debug-sessions",
            routing::post(crate::api::debug_sessions::create_session).get(crate::api::debug_sessions::list_sessions),
        )
        .route(
            "/api/debug-sessions/{id}",
            routing::get(crate::api::debug_sessions::get_session),
        )
        .route(
            "/api/debug-sessions/{id}/end",
            routing::post(crate::api::debug_sessions::end_session),
        )
        .route(
            "/api/peers/{peer_id}/channels",
            routing::get(crate::api::channels::get_peer_channels),
        )
        // SSE live event stream
        .route("/api/events", routing::get(crate::api::events::event_stream))
        // Static file serving (CSS, JS)
        .nest_service("/static", ServeDir::new("static"))
        .with_state(state)
        .layer(TraceLayer::new_for_http())
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

    let listener = TcpListener::bind(addr).await?;
    axum::serve(listener, router).await?;

    Ok(())
}
