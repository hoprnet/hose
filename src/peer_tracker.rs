use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::types::Peer;

/// In-memory registry tracking which peers have sent telemetry.
/// Thread-safe via RwLock for concurrent access from gRPC handlers.
#[derive(Debug, Clone)]
pub struct PeerTracker {
    peers: Arc<RwLock<HashMap<String, Peer>>>,
}

impl PeerTracker {
    pub fn new() -> Self {
        Self {
            peers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Record that a peer was seen. Updates last_seen if already tracked.
    pub async fn record_seen(&self, peer_id: &str) {
        let now = Utc::now();
        let mut peers = self.peers.write().await;
        peers
            .entry(peer_id.to_string())
            .and_modify(|p| p.last_seen = now)
            .or_insert_with(|| Peer {
                peer_id: peer_id.to_string(),
                last_seen: now,
            });
    }

    /// Get a snapshot of all tracked peers.
    pub async fn list_peers(&self) -> Vec<Peer> {
        let peers = self.peers.read().await;
        let mut list: Vec<Peer> = peers.values().cloned().collect();
        list.sort_by(|a, b| a.peer_id.cmp(&b.peer_id));
        list
    }

    /// Get a specific peer by ID.
    pub async fn get_peer(&self, peer_id: &str) -> Option<Peer> {
        let peers = self.peers.read().await;
        peers.get(peer_id).cloned()
    }

    /// Get the total number of tracked peers.
    pub async fn peer_count(&self) -> usize {
        self.peers.read().await.len()
    }

    /// Check if a specific peer has been seen.
    pub async fn is_tracked(&self, peer_id: &str) -> bool {
        self.peers.read().await.contains_key(peer_id)
    }
}

impl Default for PeerTracker {
    fn default() -> Self {
        Self::new()
    }
}
