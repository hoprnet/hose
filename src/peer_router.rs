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
