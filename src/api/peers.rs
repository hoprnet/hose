use axum::extract::State;
use axum::Json;

use crate::server::AppState;
use crate::types::Peer;

/// GET /api/peers - List all tracked peers.
#[tracing::instrument(skip(state))]
pub async fn list_peers(State(state): State<AppState>) -> Json<Vec<Peer>> {
    let peers = state.peer_tracker.list_peers().await;
    tracing::debug!(count = peers.len(), "peers listed");
    Json(peers)
}
