use axum::extract::State;
use axum::Json;

use crate::server::AppState;
use crate::types::Peer;

/// GET /api/peers - List all tracked peers.
pub async fn list_peers(State(state): State<AppState>) -> Json<Vec<Peer>> {
    Json(state.peer_tracker.list_peers().await)
}
