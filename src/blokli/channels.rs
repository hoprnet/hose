use serde::{Deserialize, Serialize};

use super::{BlokliClient, BlokliError};

/// On-chain channel state.
#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Deserialize)]
struct ChannelsResponse {
    channels: Vec<RawChannel>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawChannel {
    id: String,
    source: String,
    destination: String,
    status: String,
    balance: String,
    channel_epoch: u64,
    ticket_index: u64,
    closure_time: Option<String>,
}

fn map_channels(raw_channels: Vec<RawChannel>) -> Vec<ChannelData> {
    raw_channels
        .into_iter()
        .map(|c| ChannelData {
            id: c.id,
            source: c.source,
            destination: c.destination,
            status: c.status,
            balance: c.balance,
            channel_epoch: c.channel_epoch,
            ticket_index: c.ticket_index,
            closure_time: c.closure_time,
        })
        .collect()
}

/// Query channels between two peers.
pub async fn query_channels(
    client: &BlokliClient,
    source_key_id: &str,
    dest_key_id: &str,
) -> Result<Vec<ChannelData>, BlokliError> {
    let query = r#"query($source: String!, $dest: String!) {
        channels(where: { source: $source, destination: $dest }) {
            id
            source
            destination
            status
            balance
            channelEpoch
            ticketIndex
            closureTime
        }
    }"#;

    let variables = serde_json::json!({
        "source": source_key_id,
        "dest": dest_key_id,
    });

    let response: ChannelsResponse = client.query(query, Some(variables)).await?;

    Ok(map_channels(response.channels))
}

/// Query all channels for a given peer (as source or destination).
pub async fn query_peer_channels(client: &BlokliClient, key_id: &str) -> Result<Vec<ChannelData>, BlokliError> {
    let query = r#"query($keyId: String!) {
        channels(where: { or: [{ source: $keyId }, { destination: $keyId }] }) {
            id
            source
            destination
            status
            balance
            channelEpoch
            ticketIndex
            closureTime
        }
    }"#;

    let variables = serde_json::json!({ "keyId": key_id });

    let response: ChannelsResponse = client.query(query, Some(variables)).await?;

    Ok(map_channels(response.channels))
}

/// Query all channels known by the indexer.
pub async fn query_all_channels(client: &BlokliClient) -> Result<Vec<ChannelData>, BlokliError> {
    let query = r#"query {
        channels {
            id
            source
            destination
            status
            balance
            channelEpoch
            ticketIndex
            closureTime
        }
    }"#;

    let response: ChannelsResponse = client.query(query, None).await?;
    Ok(map_channels(response.channels))
}
