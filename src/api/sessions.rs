use axum::extract::State;
use axum::Json;

use crate::server::AppState;
use crate::types::HoprSession;

/// GET /api/sessions - List all active HOPR sessions.
pub async fn list_sessions(State(state): State<AppState>) -> Json<Vec<HoprSession>> {
    Json(state.session_tracker.list_sessions().await)
}
