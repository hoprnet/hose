use std::{collections::BTreeMap, time::Duration};

use blokli_client::{
    api::{
        AccountSelector, BlokliQueryClient, BlokliSubscriptionClient, ChannelFilter, ChannelSelector,
        types::{Account, Channel, ChannelStatus, OpenedChannelsGraphEntry},
    },
    errors::ErrorKind,
};
use futures::StreamExt;

use super::{BlokliClient, BlokliError};

/// On-chain channel state.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChannelData {
    pub id: String,
    pub source: String,
    pub destination: String,
    pub status: String,
    pub balance: String,
    pub channel_epoch: u64,
    pub ticket_index: u64,
    pub closure_time: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_peer_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub destination_peer_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AccountIdentity {
    pub key_id: String,
    pub chain_key: Option<String>,
    pub packet_key: Option<String>,
    pub peer_id: Option<String>,
}

fn parse_key_id(key_id: &str) -> Result<u32, BlokliError> {
    key_id
        .parse::<u32>()
        .map_err(|_| BlokliError::Client(ErrorKind::ParseError.into()))
}

fn parse_packet_key(packet_key: &str) -> Result<[u8; 32], BlokliError> {
    let bytes = hex::decode(packet_key).map_err(|_| BlokliError::Client(ErrorKind::ParseError.into()))?;
    bytes
        .try_into()
        .map_err(|_| BlokliError::Client(ErrorKind::ParseError.into()))
}

pub fn peer_id_from_multi_addresses(addresses: &[String]) -> Option<String> {
    addresses.iter().find_map(|address| {
        address
            .split("/p2p/")
            .nth(1)
            .and_then(|peer_id| peer_id.split('/').next())
            .filter(|peer_id| !peer_id.is_empty())
            .map(str::to_string)
            .or_else(|| {
                if address.starts_with("12D") {
                    Some(address.clone())
                } else {
                    None
                }
            })
    })
}

fn map_channel(channel: Channel) -> ChannelData {
    let status = match channel.status {
        ChannelStatus::Open => "Open",
        ChannelStatus::PendingToClose => "PendingToClose",
        ChannelStatus::Closed => "Closed",
    }
    .to_string();

    ChannelData {
        id: channel.concrete_channel_id,
        source: channel.source.to_string(),
        destination: channel.destination.to_string(),
        status,
        balance: channel.balance.0,
        channel_epoch: channel.epoch as u64,
        ticket_index: channel.ticket_index.0.parse().unwrap_or(0),
        closure_time: channel.closure_time.map(|v| v.0),
        source_peer_id: None,
        destination_peer_id: None,
    }
}

fn peer_id_from_account(account: &Account) -> Option<String> {
    peer_id_from_multi_addresses(&account.multi_addresses)
}

fn map_graph_entry(entry: OpenedChannelsGraphEntry) -> ChannelData {
    let source_peer_id = peer_id_from_account(&entry.source);
    let destination_peer_id = peer_id_from_account(&entry.destination);
    let mut channel = map_channel(entry.channel);

    channel.source_peer_id = source_peer_id;
    channel.destination_peer_id = destination_peer_id;
    channel
}

/// Query channels between two Blokli key IDs.
pub async fn query_channels(
    client: &BlokliClient,
    source_key_id: &str,
    dest_key_id: &str,
) -> Result<Vec<ChannelData>, BlokliError> {
    let channels = client
        .query_channels(ChannelSelector {
            filter: Some(ChannelFilter::SourceAndDestinationKeyIds(
                parse_key_id(source_key_id)?,
                parse_key_id(dest_key_id)?,
            )),
            ..ChannelSelector::default()
        })
        .await?;

    Ok(channels.channels.into_iter().map(map_channel).collect())
}

/// Query all channels for a given Blokli key ID as source or destination.
pub async fn query_peer_channels(client: &BlokliClient, key_id: &str) -> Result<Vec<ChannelData>, BlokliError> {
    let key_id = parse_key_id(key_id)?;
    let mut channels = Vec::new();

    let source_channels = client
        .query_channels(ChannelSelector {
            filter: Some(ChannelFilter::SourceKeyId(key_id)),
            ..ChannelSelector::default()
        })
        .await?;
    channels.extend(source_channels.channels.into_iter().map(map_channel));

    let destination_channels = client
        .query_channels(ChannelSelector {
            filter: Some(ChannelFilter::DestinationKeyId(key_id)),
            ..ChannelSelector::default()
        })
        .await?;
    channels.extend(destination_channels.channels.into_iter().map(map_channel));

    channels.sort_by(|a, b| a.id.cmp(&b.id));
    channels.dedup_by(|a, b| a.id == b.id);

    Ok(channels)
}

/// Query all channels known by the indexer by expanding from discovered key IDs.
pub async fn query_all_channels(client: &BlokliClient) -> Result<Vec<ChannelData>, BlokliError> {
    let mut stream = client.subscribe_graph()?;
    let mut channels = BTreeMap::new();

    loop {
        let timeout = if channels.is_empty() {
            Duration::from_secs(2)
        } else {
            Duration::from_millis(150)
        };

        match tokio::time::timeout(timeout, stream.next()).await {
            Ok(Some(Ok(entry))) => {
                let channel = map_graph_entry(entry);
                channels.insert(channel.id.clone(), channel);
            }
            Ok(Some(Err(err))) => return Err(err.into()),
            Ok(None) | Err(_) => break,
        }
    }

    Ok(channels.into_values().collect())
}

/// Query Blokli account key IDs by packet key.
pub async fn query_key_ids_by_packet_key(client: &BlokliClient, packet_key: &str) -> Result<Vec<String>, BlokliError> {
    let accounts = client
        .query_accounts(AccountSelector::PacketKey(parse_packet_key(packet_key)?))
        .await?;
    Ok(accounts.into_iter().map(|a| a.keyid.to_string()).collect())
}

/// Query Blokli account identity by key ID.
pub async fn query_account_identity_by_key_id(
    client: &BlokliClient,
    key_id: &str,
) -> Result<Option<AccountIdentity>, BlokliError> {
    let mut accounts = client
        .query_accounts(AccountSelector::KeyId(parse_key_id(key_id)?))
        .await?;

    Ok(accounts.pop().map(|a| AccountIdentity {
        key_id: a.keyid.to_string(),
        chain_key: Some(a.chain_key),
        packet_key: Some(a.packet_key),
        peer_id: peer_id_from_multi_addresses(&a.multi_addresses),
    }))
}

#[cfg(test)]
mod tests {
    use super::peer_id_from_multi_addresses;

    #[test]
    fn peer_id_from_multi_addresses_extracts_p2p_component() {
        let addresses = vec!["/ip4/127.0.0.1/tcp/9091/p2p/12D3KooWSource".to_string()];

        assert_eq!(
            peer_id_from_multi_addresses(&addresses),
            Some("12D3KooWSource".to_string())
        );
    }

    #[test]
    fn peer_id_from_multi_addresses_accepts_bare_peer_id_for_compatibility() {
        let addresses = vec!["12D3KooWSource".to_string()];

        assert_eq!(
            peer_id_from_multi_addresses(&addresses),
            Some("12D3KooWSource".to_string())
        );
    }

    #[test]
    fn peer_id_from_multi_addresses_ignores_addresses_without_peer_id() {
        let addresses = vec!["/ip4/127.0.0.1/tcp/9091".to_string()];

        assert_eq!(peer_id_from_multi_addresses(&addresses), None);
    }
}
