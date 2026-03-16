use std::time::Duration;
use tokio::sync::broadcast;

use super::BlokliClient;
use super::channels::{query_peer_channels, ChannelData};

/// Event emitted when channel state changes are detected.
#[derive(Debug, Clone)]
pub struct ChannelStateChange {
    pub channel_id: String,
    pub source: String,
    pub destination: String,
    pub status: String,
    pub balance: String,
}

/// Spawn a background task that polls channel state for watched peers.
/// Emits events when changes are detected.
pub fn spawn_channel_watcher(
    client: BlokliClient,
    key_ids: Vec<String>,
    change_tx: broadcast::Sender<ChannelStateChange>,
    poll_interval: Duration,
) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(poll_interval);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        let mut last_known: std::collections::HashMap<String, ChannelData> =
            std::collections::HashMap::new();

        loop {
            interval.tick().await;

            for key_id in &key_ids {
                match query_peer_channels(&client, key_id).await {
                    Ok(channels) => {
                        for channel in channels {
                            let changed = last_known
                                .get(&channel.id)
                                .map(|prev| {
                                    prev.status != channel.status
                                        || prev.balance != channel.balance
                                        || prev.ticket_index != channel.ticket_index
                                })
                                .unwrap_or(true);

                            if changed {
                                let _ = change_tx.send(ChannelStateChange {
                                    channel_id: channel.id.clone(),
                                    source: channel.source.clone(),
                                    destination: channel.destination.clone(),
                                    status: channel.status.clone(),
                                    balance: channel.balance.clone(),
                                });
                                last_known.insert(channel.id.clone(), channel);
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!(key_id, error = %e, "failed to poll channel state");
                    }
                }
            }
        }
    });
}
