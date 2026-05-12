use std::collections::{BTreeMap, BTreeSet};

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::Deserialize;

use crate::{
    blokli::channels::{ChannelData, query_all_channels, query_peer_channels},
    server::AppState,
};

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FilterMode {
    Any,
    Both,
}

impl Default for FilterMode {
    fn default() -> Self {
        Self::Any
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ChannelsQuery {
    #[serde(default)]
    pub peer_ids: Vec<String>,
    #[serde(default)]
    pub filter_mode: FilterMode,
}

pub fn parse_peer_ids(raw_peer_ids: &[String]) -> Vec<String> {
    let mut deduped = BTreeSet::new();
    for raw in raw_peer_ids {
        for token in raw.split([',', '\n', '\r', '\t', ' ']) {
            let value = token.trim();
            if !value.is_empty() {
                deduped.insert(value.to_string());
            }
        }
    }
    deduped.into_iter().collect()
}

pub fn apply_filter(channels: Vec<ChannelData>, selected_keys: &[String], mode: FilterMode) -> Vec<ChannelData> {
    if selected_keys.is_empty() {
        return channels;
    }

    let selected: BTreeSet<&str> = selected_keys.iter().map(String::as_str).collect();

    channels
        .into_iter()
        .filter(|channel| match mode {
            FilterMode::Any => {
                selected.contains(channel.source.as_str()) || selected.contains(channel.destination.as_str())
            }
            FilterMode::Both => {
                selected.contains(channel.source.as_str()) && selected.contains(channel.destination.as_str())
            }
        })
        .collect()
}

/// GET /api/channels - Query channel graph with optional peer filtering.
pub async fn get_channels(
    State(state): State<AppState>,
    Query(query): Query<ChannelsQuery>,
) -> Result<Json<Vec<ChannelData>>, (StatusCode, String)> {
    let blokli_client = state.blokli_client.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Blockchain indexer not configured".to_string(),
        )
    })?;

    let peer_ids = parse_peer_ids(&query.peer_ids);

    // Convert peer IDs to key IDs when known, but keep original values as fallback.
    let mut selected_keys: Vec<String> = Vec::with_capacity(peer_ids.len());
    for peer_id in &peer_ids {
        if let Some(key_id) = state.identity_bridge.key_id_for_peer(peer_id).await {
            selected_keys.push(key_id);
        } else {
            selected_keys.push(peer_id.clone());
        }
    }

    let mut channels = if selected_keys.is_empty() {
        query_all_channels(blokli_client)
            .await
            .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Indexer query failed: {e}")))?
    } else {
        let mut deduped = BTreeMap::new();

        for key_id in &selected_keys {
            let peer_channels = query_peer_channels(blokli_client, key_id)
                .await
                .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Indexer query failed: {e}")))?;

            for channel in peer_channels {
                deduped.insert(channel.id.clone(), channel);
            }
        }

        deduped.into_values().collect()
    };

    channels = apply_filter(channels, &selected_keys, query.filter_mode);
    channels.sort_by(|a, b| a.id.cmp(&b.id));

    Ok(Json(channels))
}

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

    let key_id = state.identity_bridge.key_id_for_peer(&peer_id).await.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            format!("No blockchain key found for peer {peer_id}"),
        )
    })?;

    let channels = query_peer_channels(blokli_client, &key_id)
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Indexer query failed: {e}")))?;

    Ok(Json(channels))
}

#[cfg(test)]
mod tests {
    use super::{FilterMode, apply_filter, parse_peer_ids};
    use crate::blokli::channels::ChannelData;

    fn ch(id: &str, source: &str, destination: &str) -> ChannelData {
        ChannelData {
            id: id.to_string(),
            source: source.to_string(),
            destination: destination.to_string(),
            status: "Open".to_string(),
            balance: "0".to_string(),
            channel_epoch: 0,
            ticket_index: 0,
            closure_time: None,
        }
    }

    #[test]
    fn parse_peer_ids_splits_and_dedupes() {
        let parsed = parse_peer_ids(&[
            "peer-a,peer-b".to_string(),
            "peer-b\npeer-c".to_string(),
            " peer-c\tpeer-d ".to_string(),
        ]);

        assert_eq!(parsed, vec!["peer-a", "peer-b", "peer-c", "peer-d"]);
    }

    #[test]
    fn filter_mode_any_keeps_channels_with_either_endpoint() {
        let channels = vec![ch("1", "k1", "k2"), ch("2", "k3", "k4"), ch("3", "k2", "k9")];

        let filtered = apply_filter(channels, &["k2".to_string(), "k8".to_string()], FilterMode::Any);
        let ids: Vec<String> = filtered.into_iter().map(|c| c.id).collect();

        assert_eq!(ids, vec!["1", "3"]);
    }

    #[test]
    fn filter_mode_both_keeps_only_induced_subgraph_edges() {
        let channels = vec![ch("1", "k1", "k2"), ch("2", "k2", "k3"), ch("3", "k9", "k2")];

        let filtered = apply_filter(
            channels,
            &["k1".to_string(), "k2".to_string(), "k3".to_string()],
            FilterMode::Both,
        );
        let ids: Vec<String> = filtered.into_iter().map(|c| c.id).collect();

        assert_eq!(ids, vec!["1", "2"]);
    }
}
