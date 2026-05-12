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
    channels: ChannelsResult,
}

#[derive(Deserialize)]
struct ChannelsResult {
    #[serde(rename = "__typename")]
    typename: String,
    channels: Option<Vec<RawChannel>>,
    message: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawChannel {
    concrete_channel_id: String,
    source: i64,
    destination: i64,
    status: String,
    balance: String,
    epoch: u64,
    ticket_index: String,
    closure_time: Option<String>,
}

#[derive(Deserialize)]
struct SafesResponse {
    safes: SafesResult,
}

#[derive(Deserialize)]
struct SafesResult {
    #[serde(rename = "__typename")]
    typename: String,
    safes: Option<Vec<SafeData>>,
    message: Option<String>,
}

#[derive(Deserialize)]
struct SafeData {
    address: String,
}

fn channels_from_result(result: ChannelsResult) -> Result<Vec<ChannelData>, BlokliError> {
    match result.typename.as_str() {
        "ChannelsList" => Ok(map_channels(result.channels.unwrap_or_default())),
        _ => Err(BlokliError::GraphQL(result.message.unwrap_or_else(|| {
            format!("unexpected channels response type '{}'", result.typename)
        }))),
    }
}

fn safes_from_result(result: SafesResult) -> Result<Vec<SafeData>, BlokliError> {
    match result.typename.as_str() {
        "SafesList" => Ok(result.safes.unwrap_or_default()),
        _ => Err(BlokliError::GraphQL(result.message.unwrap_or_else(|| {
            format!("unexpected safes response type '{}'", result.typename)
        }))),
    }
}

fn parse_key_id(key_id: &str) -> Result<i64, BlokliError> {
    key_id
        .parse()
        .map_err(|_| BlokliError::GraphQL(format!("invalid Blokli key ID '{key_id}'")))
}

fn map_channels(raw_channels: Vec<RawChannel>) -> Vec<ChannelData> {
    raw_channels
        .into_iter()
        .map(|c| ChannelData {
            id: c.concrete_channel_id,
            source: c.source.to_string(),
            destination: c.destination.to_string(),
            status: c.status,
            balance: c.balance,
            channel_epoch: c.epoch,
            ticket_index: c.ticket_index.parse().unwrap_or(0),
            closure_time: c.closure_time,
        })
        .collect()
}

/// Query channels between two Blokli key IDs.
pub async fn query_channels(
    client: &BlokliClient,
    source_key_id: &str,
    dest_key_id: &str,
) -> Result<Vec<ChannelData>, BlokliError> {
    let query = r#"query($sourceKeyId: Int, $destinationKeyId: Int) {
        channels(sourceKeyId: $sourceKeyId, destinationKeyId: $destinationKeyId) {
            __typename
            ... on ChannelsList {
                channels {
                    concreteChannelId
                    source
                    destination
                    status
                    balance
                    epoch
                    ticketIndex
                    closureTime
                }
            }
            ... on InvalidAddressError { message }
            ... on MissingFilterError { message }
            ... on QueryFailedError { message }
        }
    }"#;

    let variables = serde_json::json!({
        "sourceKeyId": parse_key_id(source_key_id)?,
        "destinationKeyId": parse_key_id(dest_key_id)?,
    });

    let response: ChannelsResponse = client.query(query, Some(variables)).await?;
    channels_from_result(response.channels)
}

/// Query all channels for a given Blokli key ID as source or destination.
pub async fn query_peer_channels(client: &BlokliClient, key_id: &str) -> Result<Vec<ChannelData>, BlokliError> {
    let key_id = parse_key_id(key_id)?;
    let mut channels = Vec::new();

    channels.extend(query_channels_by_endpoint(client, "sourceKeyId", key_id).await?);
    channels.extend(query_channels_by_endpoint(client, "destinationKeyId", key_id).await?);

    channels.sort_by(|a, b| a.id.cmp(&b.id));
    channels.dedup_by(|a, b| a.id == b.id);

    Ok(channels)
}

/// Query all channels known by the indexer by expanding the safes list.
pub async fn query_all_channels(client: &BlokliClient) -> Result<Vec<ChannelData>, BlokliError> {
    let query = r#"query {
        safes {
            __typename
            ... on SafesList { safes { address } }
            ... on QueryFailedError { message }
        }
    }"#;

    let response: SafesResponse = client.query(query, None).await?;
    let safes = safes_from_result(response.safes)?;
    let mut channels = Vec::new();

    for safe in safes {
        channels.extend(query_safe_channels(client, &safe.address).await?);
    }

    channels.sort_by(|a, b| a.id.cmp(&b.id));
    channels.dedup_by(|a, b| a.id == b.id);

    Ok(channels)
}

async fn query_channels_by_endpoint(
    client: &BlokliClient,
    key_arg: &str,
    key_id: i64,
) -> Result<Vec<ChannelData>, BlokliError> {
    let query = format!(
        r#"query($keyId: Int) {{
            channels({key_arg}: $keyId) {{
                __typename
                ... on ChannelsList {{
                    channels {{
                        concreteChannelId
                        source
                        destination
                        status
                        balance
                        epoch
                        ticketIndex
                        closureTime
                    }}
                }}
                ... on InvalidAddressError {{ message }}
                ... on MissingFilterError {{ message }}
                ... on QueryFailedError {{ message }}
            }}
        }}"#
    );

    let variables = serde_json::json!({ "keyId": key_id });
    let response: ChannelsResponse = client.query(&query, Some(variables)).await?;
    channels_from_result(response.channels)
}

async fn query_safe_channels(client: &BlokliClient, safe_address: &str) -> Result<Vec<ChannelData>, BlokliError> {
    let query = r#"query($safeAddress: String) {
        channels(safeAddress: $safeAddress) {
            __typename
            ... on ChannelsList {
                channels {
                    concreteChannelId
                    source
                    destination
                    status
                    balance
                    epoch
                    ticketIndex
                    closureTime
                }
            }
            ... on InvalidAddressError { message }
            ... on MissingFilterError { message }
            ... on QueryFailedError { message }
        }
    }"#;

    let variables = serde_json::json!({ "safeAddress": safe_address });
    let response: ChannelsResponse = client.query(query, Some(variables)).await?;
    channels_from_result(response.channels)
}
