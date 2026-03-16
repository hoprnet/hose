use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;

use crate::blokli::channels::{ChannelData, query_peer_channels};
use crate::server::AppState;

/// GET /api/peers/:peer_id/channels - Query on-chain channels for a peer.
pub async fn get_peer_channels(
    State(state): State<AppState>,
    Path(peer_id): Path<String>,
) -> Result<Json<Vec<ChannelData>>, (StatusCode, String)> {
    let blokli_client = state.blokli_client.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Blockchain indexer not configured".to_string(),
        )
    })?;

    let key_id = state
        .identity_bridge
        .key_id_for_peer(&peer_id)
        .await
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("No blockchain key found for peer {peer_id}"),
            )
        })?;

    let channels = query_peer_channels(blokli_client, &key_id)
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_GATEWAY,
                format!("Indexer query failed: {e}"),
            )
        })?;

    Ok(Json(channels))
}
