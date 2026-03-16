use axum::Json;
use axum::extract::State;

use crate::server::AppState;
use crate::types::HoprSession;

/// GET /api/sessions - List all active HOPR sessions.
#[tracing::instrument(skip(state))]
pub async fn list_sessions(State(state): State<AppState>) -> Json<Vec<HoprSession>> {
    let sessions = state.session_tracker.list_sessions().await;
    tracing::debug!(count = sessions.len(), "HOPR sessions listed");
    Json(sessions)
}
