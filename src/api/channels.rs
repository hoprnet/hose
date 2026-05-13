use std::collections::{BTreeMap, BTreeSet};

use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
};
use serde::{Deserialize, Serialize};

use crate::{
    blokli::channels::{
        AccountIdentity, ChannelData, query_account_identity_by_key_id, query_all_channels,
        query_key_ids_by_packet_key, query_peer_channels,
    },
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
    #[serde(default, deserialize_with = "deserialize_peer_ids")]
    pub peer_ids: Vec<String>,
    #[serde(default)]
    pub filter_mode: FilterMode,
}

fn deserialize_peer_ids<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum PeerIdsInput {
        One(String),
        Many(Vec<String>),
    }

    let parsed = Option::<PeerIdsInput>::deserialize(deserializer)?;
    Ok(match parsed {
        Some(PeerIdsInput::One(v)) => vec![v],
        Some(PeerIdsInput::Many(v)) => v,
        None => Vec::new(),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FilterTokenKind {
    KeyId,
    PacketKeyHex,
    PeerId,
}

pub fn parse_filter_terms(raw_terms: &[String]) -> Vec<String> {
    let mut deduped = BTreeSet::new();
    for raw in raw_terms {
        for token in raw.split([',', '\n', '\r', '\t', ' ']) {
            let value = token.trim();
            if !value.is_empty() {
                deduped.insert(value.to_string());
            }
        }
    }
    deduped.into_iter().collect()
}

fn classify_token(token: &str) -> FilterTokenKind {
    if token.chars().all(|c| c.is_ascii_digit()) {
        return FilterTokenKind::KeyId;
    }
    if token.starts_with("0x") && token.len() > 2 && token.chars().skip(2).all(|c| c.is_ascii_hexdigit()) {
        return FilterTokenKind::PacketKeyHex;
    }
    FilterTokenKind::PeerId
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

#[derive(Debug, Clone, Serialize)]
pub struct ChannelGraphRow {
    pub id: String,
    pub source: String,
    pub destination: String,
    pub source_key_id: String,
    pub destination_key_id: String,
    pub source_peer_id: Option<String>,
    pub destination_peer_id: Option<String>,
    pub source_packet_key: Option<String>,
    pub destination_packet_key: Option<String>,
    pub status: String,
    pub balance: String,
    pub channel_epoch: u64,
    pub ticket_index: u64,
    pub closure_time: Option<String>,
}

/// GET /api/channels - Query channel graph with optional peer filtering.
pub async fn get_channels(
    State(state): State<AppState>,
    Query(query): Query<ChannelsQuery>,
) -> Result<(HeaderMap, Json<Vec<ChannelGraphRow>>), (StatusCode, String)> {
    let blokli_client = state.blokli_client.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Blockchain indexer not configured".to_string(),
        )
    })?;

    let terms = parse_filter_terms(&query.peer_ids);

    let mut selected_keys = BTreeSet::new();
    let mut unresolved = Vec::new();

    for token in &terms {
        match classify_token(token) {
            FilterTokenKind::KeyId => {
                selected_keys.insert(token.clone());
            }
            FilterTokenKind::PacketKeyHex => {
                let packet_key = token.trim_start_matches("0x");
                let resolved = query_key_ids_by_packet_key(blokli_client, packet_key)
                    .await
                    .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Indexer query failed: {e}")))?;
                if resolved.is_empty() {
                    unresolved.push(token.clone());
                } else {
                    for key in resolved {
                        selected_keys.insert(key);
                    }
                }
            }
            FilterTokenKind::PeerId => {
                if let Some(key_id) = state.identity_bridge.key_id_for_peer(token).await {
                    selected_keys.insert(key_id);
                } else {
                    unresolved.push(token.clone());
                }
            }
        }
    }

    let selected_keys: Vec<String> = selected_keys.into_iter().collect();

    let mut channels = if terms.is_empty() {
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

    // Resolve endpoint identity fields for current result-set key IDs.
    let mut unique_keys = BTreeSet::new();
    for channel in &channels {
        unique_keys.insert(channel.source.clone());
        unique_keys.insert(channel.destination.clone());
    }

    let mut account_map: BTreeMap<String, Option<AccountIdentity>> = BTreeMap::new();
    for key_id in unique_keys {
        let account = query_account_identity_by_key_id(blokli_client, &key_id)
            .await
            .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Indexer query failed: {e}")))?;
        account_map.insert(key_id, account);
    }

    let mut rows = Vec::with_capacity(channels.len());
    for channel in channels {
        let source_key_id = channel.source.clone();
        let destination_key_id = channel.destination.clone();

        let source_packet_key = account_map
            .get(&source_key_id)
            .and_then(|entry| entry.as_ref())
            .and_then(|identity| identity.packet_key.clone());
        let destination_packet_key = account_map
            .get(&destination_key_id)
            .and_then(|entry| entry.as_ref())
            .and_then(|identity| identity.packet_key.clone());

        let source_peer_id = state
            .identity_bridge
            .cached_peer_id_for_key(&source_key_id)
            .await
            .or_else(|| {
                account_map
                    .get(&source_key_id)
                    .and_then(|entry| entry.as_ref())
                    .and_then(|identity| identity.chain_key.clone())
            });
        let destination_peer_id = state
            .identity_bridge
            .cached_peer_id_for_key(&destination_key_id)
            .await
            .or_else(|| {
                account_map
                    .get(&destination_key_id)
                    .and_then(|entry| entry.as_ref())
                    .and_then(|identity| identity.chain_key.clone())
            });

        rows.push(ChannelGraphRow {
            id: channel.id,
            source: source_key_id.clone(),
            destination: destination_key_id.clone(),
            source_key_id,
            destination_key_id,
            source_peer_id,
            destination_peer_id,
            source_packet_key,
            destination_packet_key,
            status: channel.status,
            balance: channel.balance,
            channel_epoch: channel.channel_epoch,
            ticket_index: channel.ticket_index,
            closure_time: channel.closure_time,
        });
    }

    let mut headers = HeaderMap::new();
    if let Ok(value) = HeaderValue::from_str(&unresolved.len().to_string()) {
        headers.insert("x-hose-filter-unresolved-count", value);
    }
    if !unresolved.is_empty() {
        let compact = unresolved.join(",");
        if let Ok(value) = compact.parse() {
            headers.insert("x-hose-filter-unresolved", value);
        }
    }

    Ok((headers, Json(rows)))
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
    use super::{FilterMode, FilterTokenKind, apply_filter, classify_token, parse_filter_terms};
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
    fn parse_filter_terms_splits_and_dedupes() {
        let parsed = parse_filter_terms(&[
            "peer-a,peer-b".to_string(),
            "peer-b\npeer-c".to_string(),
            " peer-c\tpeer-d ".to_string(),
        ]);

        assert_eq!(parsed, vec!["peer-a", "peer-b", "peer-c", "peer-d"]);
    }

    #[test]
    fn classify_token_detects_key_types() {
        assert_eq!(classify_token("42"), FilterTokenKind::KeyId);
        assert_eq!(classify_token("0xabcdef0123456789"), FilterTokenKind::PacketKeyHex);
        assert_eq!(classify_token("12D3KooWabc"), FilterTokenKind::PeerId);
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
