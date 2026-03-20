use axum::{Json, extract::State};

use crate::{server::AppState, types::Peer};

/// GET /api/peers - List all tracked peers.
#[tracing::instrument(skip(state))]
pub async fn list_peers(State(state): State<AppState>) -> Json<Vec<Peer>> {
    let peers = state.peer_tracker.list_peers().await;
    tracing::debug!(count = peers.len(), "peers listed");
    Json(peers)
}
