use std::{collections::HashMap, sync::Arc};

use chrono::Utc;
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
        let is_new = !peers.contains_key(peer_id);
        peers
            .entry(peer_id.to_string())
            .and_modify(|p| p.last_seen = now)
            .or_insert_with(|| Peer {
                peer_id: peer_id.to_string(),
                last_seen: now,
            });
        if is_new {
            tracing::info!(peer_id = %peer_id, "peer first seen");
        }
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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[tokio::test]
    async fn record_seen_adds_a_peer() {
        let tracker = PeerTracker::new();
        tracker.record_seen("peer-1").await;

        let peer = tracker.get_peer("peer-1").await;
        assert!(peer.is_some());
        assert_eq!(peer.unwrap().peer_id, "peer-1");
    }

    #[tokio::test]
    async fn record_seen_updates_last_seen_for_existing_peer() {
        let tracker = PeerTracker::new();
        tracker.record_seen("peer-1").await;
        let first_seen = tracker.get_peer("peer-1").await.unwrap().last_seen;

        // Small delay to ensure timestamp differs
        tokio::time::sleep(Duration::from_millis(10)).await;

        tracker.record_seen("peer-1").await;
        let second_seen = tracker.get_peer("peer-1").await.unwrap().last_seen;

        assert!(second_seen > first_seen);
        // Should still be one peer, not two
        assert_eq!(tracker.peer_count().await, 1);
    }

    #[tokio::test]
    async fn list_peers_returns_sorted_by_peer_id() {
        let tracker = PeerTracker::new();
        tracker.record_seen("charlie").await;
        tracker.record_seen("alice").await;
        tracker.record_seen("bob").await;

        let peers = tracker.list_peers().await;
        let ids: Vec<&str> = peers.iter().map(|p| p.peer_id.as_str()).collect();
        assert_eq!(ids, vec!["alice", "bob", "charlie"]);
    }

    #[tokio::test]
    async fn get_peer_returns_none_for_unknown() {
        let tracker = PeerTracker::new();
        assert!(tracker.get_peer("nonexistent").await.is_none());
    }

    #[tokio::test]
    async fn peer_count_reflects_tracked_peers() {
        let tracker = PeerTracker::new();
        assert_eq!(tracker.peer_count().await, 0);

        tracker.record_seen("peer-1").await;
        assert_eq!(tracker.peer_count().await, 1);

        tracker.record_seen("peer-2").await;
        assert_eq!(tracker.peer_count().await, 2);

        // Duplicate should not increase count
        tracker.record_seen("peer-1").await;
        assert_eq!(tracker.peer_count().await, 2);
    }

    #[tokio::test]
    async fn is_tracked_returns_correct_status() {
        let tracker = PeerTracker::new();
        assert!(!tracker.is_tracked("peer-1").await);

        tracker.record_seen("peer-1").await;
        assert!(tracker.is_tracked("peer-1").await);
        assert!(!tracker.is_tracked("peer-2").await);
    }
}
