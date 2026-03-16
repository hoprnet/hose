use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::types::RoutingDecision;

/// Routes incoming telemetry per peer: discard (default) or retain for active debug sessions.
#[derive(Debug, Clone)]
pub struct PeerRouter {
    /// Maps peer_id -> set of debug session IDs retaining that peer's telemetry.
    routes: Arc<RwLock<HashMap<String, HashSet<Uuid>>>>,
}

impl PeerRouter {
    pub fn new() -> Self {
        Self {
            routes: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a debug session's interest in a set of peers.
    pub async fn add_session(&self, session_id: Uuid, peer_ids: &[String]) {
        let mut routes = self.routes.write().await;
        for peer_id in peer_ids {
            routes.entry(peer_id.clone()).or_default().insert(session_id);
        }
    }

    /// Remove a debug session's interest. Cleans up peers with no remaining sessions.
    pub async fn remove_session(&self, session_id: Uuid) {
        let mut routes = self.routes.write().await;
        routes.retain(|_, sessions| {
            sessions.remove(&session_id);
            !sessions.is_empty()
        });
    }

    /// Get the routing decision for a peer.
    pub async fn route(&self, peer_id: &str) -> RoutingDecision {
        let routes = self.routes.read().await;
        match routes.get(peer_id) {
            Some(sessions) if !sessions.is_empty() => RoutingDecision::Retain {
                session_ids: sessions.iter().copied().collect(),
            },
            _ => RoutingDecision::Discard,
        }
    }

    /// Check if any peer is currently in retain mode.
    pub async fn has_retained_peers(&self) -> bool {
        !self.routes.read().await.is_empty()
    }
}

impl Default for PeerRouter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn default_routing_is_discard() {
        let router = PeerRouter::new();
        assert!(matches!(router.route("peer-1").await, RoutingDecision::Discard));
    }

    #[tokio::test]
    async fn add_session_enables_retain_for_peers() {
        let router = PeerRouter::new();
        let session_id = Uuid::new_v4();
        router
            .add_session(session_id, &["peer-1".to_string(), "peer-2".to_string()])
            .await;

        match router.route("peer-1").await {
            RoutingDecision::Retain { session_ids } => {
                assert_eq!(session_ids.len(), 1);
                assert!(session_ids.contains(&session_id));
            }
            RoutingDecision::Discard => panic!("expected Retain, got Discard"),
        }

        match router.route("peer-2").await {
            RoutingDecision::Retain { session_ids } => {
                assert!(session_ids.contains(&session_id));
            }
            RoutingDecision::Discard => panic!("expected Retain, got Discard"),
        }
    }

    #[tokio::test]
    async fn remove_session_reverts_to_discard() {
        let router = PeerRouter::new();
        let session_id = Uuid::new_v4();
        router
            .add_session(session_id, &["peer-1".to_string()])
            .await;

        router.remove_session(session_id).await;

        assert!(matches!(router.route("peer-1").await, RoutingDecision::Discard));
    }

    #[tokio::test]
    async fn multiple_sessions_for_same_peer() {
        let router = PeerRouter::new();
        let s1 = Uuid::new_v4();
        let s2 = Uuid::new_v4();

        router.add_session(s1, &["peer-1".to_string()]).await;
        router.add_session(s2, &["peer-1".to_string()]).await;

        match router.route("peer-1").await {
            RoutingDecision::Retain { session_ids } => {
                assert_eq!(session_ids.len(), 2);
                assert!(session_ids.contains(&s1));
                assert!(session_ids.contains(&s2));
            }
            RoutingDecision::Discard => panic!("expected Retain, got Discard"),
        }
    }

    #[tokio::test]
    async fn remove_one_session_still_retains_for_other() {
        let router = PeerRouter::new();
        let s1 = Uuid::new_v4();
        let s2 = Uuid::new_v4();

        router.add_session(s1, &["peer-1".to_string()]).await;
        router.add_session(s2, &["peer-1".to_string()]).await;

        router.remove_session(s1).await;

        match router.route("peer-1").await {
            RoutingDecision::Retain { session_ids } => {
                assert_eq!(session_ids.len(), 1);
                assert!(session_ids.contains(&s2));
            }
            RoutingDecision::Discard => panic!("expected Retain after removing only one session"),
        }
    }

    #[tokio::test]
    async fn has_retained_peers_reflects_state() {
        let router = PeerRouter::new();
        assert!(!router.has_retained_peers().await);

        let session_id = Uuid::new_v4();
        router
            .add_session(session_id, &["peer-1".to_string()])
            .await;
        assert!(router.has_retained_peers().await);

        router.remove_session(session_id).await;
        assert!(!router.has_retained_peers().await);
    }
}
