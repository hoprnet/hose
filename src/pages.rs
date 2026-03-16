use askama::Template;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};

use crate::db;
use crate::db::telemetry::{LogRow, MetricRow, PaginationParams, SpanRow};
use crate::server::AppState;
use crate::types::{DebugSession, DebugSessionStatus, HoprSession, Peer};

/// Render an Askama template into an Axum response.
fn render_template(template: &impl Template) -> Response {
    match template.render() {
        Ok(html) => Html(html).into_response(),
        Err(err) => {
            tracing::error!(%err, "template render failed");
            (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// Dashboard
// ---------------------------------------------------------------------------

#[derive(Template)]
#[template(path = "dashboard.html")]
struct DashboardTemplate {
    peer_count: usize,
    session_count: usize,
    debug_session_count: usize,
}

/// GET / - Dashboard overview page.
pub async fn dashboard(State(state): State<AppState>) -> Response {
    let peer_count = state.peer_tracker.peer_count().await;
    let session_count = state.session_tracker.session_count().await;
    let debug_session_count = db::debug_sessions::list_debug_sessions(&state.db)
        .await
        .map(|s| s.len())
        .unwrap_or(0);

    render_template(&DashboardTemplate {
        peer_count,
        session_count,
        debug_session_count,
    })
}

// ---------------------------------------------------------------------------
// Peers
// ---------------------------------------------------------------------------

#[derive(Template)]
#[template(path = "peers.html")]
struct PeersTemplate {
    peers: Vec<Peer>,
}

/// GET /peers - List all tracked peers.
pub async fn peers(State(state): State<AppState>) -> Response {
    let peers = state.peer_tracker.list_peers().await;
    render_template(&PeersTemplate { peers })
}

// ---------------------------------------------------------------------------
// Sessions
// ---------------------------------------------------------------------------

#[derive(Template)]
#[template(path = "sessions.html")]
struct SessionsTemplate {
    sessions: Vec<HoprSession>,
}

/// GET /sessions - List all HOPR sessions.
pub async fn sessions(State(state): State<AppState>) -> Response {
    let sessions = state.session_tracker.list_sessions().await;
    render_template(&SessionsTemplate { sessions })
}

// ---------------------------------------------------------------------------
// Debug Sessions
// ---------------------------------------------------------------------------

#[derive(Template)]
#[template(path = "debug_sessions.html")]
struct DebugSessionsTemplate {
    sessions: Vec<DebugSession>,
}

/// GET /debug-sessions - List all debug sessions.
pub async fn debug_sessions(State(state): State<AppState>) -> Response {
    let sessions = db::debug_sessions::list_debug_sessions(&state.db)
        .await
        .unwrap_or_default();

    render_template(&DebugSessionsTemplate { sessions })
}

// ---------------------------------------------------------------------------
// Debug Session Detail
// ---------------------------------------------------------------------------

#[derive(Template)]
#[template(path = "debug_session_detail.html")]
struct DebugSessionDetailTemplate {
    session: DebugSession,
    spans: Vec<SpanRow>,
    span_count: i64,
    metrics: Vec<MetricRow>,
    metric_count: i64,
    logs: Vec<LogRow>,
    log_count: i64,
}

/// GET /debug-sessions/:id - Detail view for a single debug session.
pub async fn debug_session_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    let session_id: uuid::Uuid = match id.parse() {
        Ok(id) => id,
        Err(_) => {
            return (StatusCode::BAD_REQUEST, "invalid session ID").into_response();
        }
    };

    let session = match db::debug_sessions::get_debug_session(&state.db, session_id).await {
        Ok(Some(s)) => s,
        Ok(None) => {
            return (StatusCode::NOT_FOUND, "debug session not found").into_response();
        }
        Err(err) => {
            tracing::error!(%err, "failed to load debug session");
            return (StatusCode::INTERNAL_SERVER_ERROR, "database error").into_response();
        }
    };

    let pagination = PaginationParams::default();

    let spans_result = db::telemetry::query_spans(&state.db, session_id, &pagination).await;
    let metrics_result = db::telemetry::query_metrics(&state.db, session_id, &pagination).await;
    let logs_result = db::telemetry::query_logs(&state.db, session_id, &pagination).await;

    let (spans, span_count) = spans_result
        .map(|r| (r.items, r.total))
        .unwrap_or_default();
    let (metrics, metric_count) = metrics_result
        .map(|r| (r.items, r.total))
        .unwrap_or_default();
    let (logs, log_count) = logs_result
        .map(|r| (r.items, r.total))
        .unwrap_or_default();

    render_template(&DebugSessionDetailTemplate {
        session,
        spans,
        span_count,
        metrics,
        metric_count,
        logs,
        log_count,
    })
}
