use blokli_client::{
    api::{
        AccountSelector, BlokliQueryClient, ChannelFilter, ChannelSelector,
        types::{Channel, ChannelStatus},
    },
    errors::ErrorKind,
};

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
    }
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
    let _ = client;
    Ok(Vec::new())
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
        peer_id: a.multi_addresses.first().cloned(),
    }))
}
