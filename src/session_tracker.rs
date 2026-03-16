use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::types::{HoprSession, SessionParticipant};

/// In-memory registry of active HOPR sessions observed in telemetry.
#[derive(Debug, Clone)]
pub struct SessionTracker {
    sessions: Arc<RwLock<HashMap<String, HoprSession>>>,
}

impl SessionTracker {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Update or insert a session with new participant information.
    /// If the session already exists, merges participants and updates last_seen.
    pub async fn update_session(
        &self,
        session_id: &str,
        protocol: &str,
        hop_count: u32,
        participant: SessionParticipant,
    ) {
        let now = Utc::now();
        let mut sessions = self.sessions.write().await;
        let is_new = !sessions.contains_key(session_id);

        sessions
            .entry(session_id.to_string())
            .and_modify(|s| {
                s.last_seen = now;
                s.hop_count = hop_count;
                // Merge participant: update if same peer_id, insert if new
                if let Some(existing) = s
                    .participants
                    .iter_mut()
                    .find(|p| p.peer_id == participant.peer_id)
                {
                    existing.role = participant.role;
                } else {
                    s.participants.push(participant.clone());
                }
            })
            .or_insert_with(|| HoprSession {
                session_id: session_id.to_string(),
                protocol: protocol.to_string(),
                hop_count,
                participants: vec![participant],
                first_seen: now,
                last_seen: now,
            });

        if is_new {
            tracing::info!(
                session_id = %session_id,
                protocol = %protocol,
                hop_count = hop_count,
                "HOPR session first seen"
            );
        }
    }

    /// Get a snapshot of all active sessions.
    pub async fn list_sessions(&self) -> Vec<HoprSession> {
        let sessions = self.sessions.read().await;
        let mut list: Vec<HoprSession> = sessions.values().cloned().collect();
        list.sort_by(|a, b| b.last_seen.cmp(&a.last_seen));
        list
    }

    /// Get a specific session by ID.
    pub async fn get_session(&self, session_id: &str) -> Option<HoprSession> {
        self.sessions.read().await.get(session_id).cloned()
    }

    /// Get all peer IDs participating in a session.
    pub async fn get_session_peers(&self, session_id: &str) -> Vec<String> {
        self.sessions
            .read()
            .await
            .get(session_id)
            .map(|s| s.participants.iter().map(|p| p.peer_id.clone()).collect())
            .unwrap_or_default()
    }

    /// Get the total number of tracked sessions.
    pub async fn session_count(&self) -> usize {
        self.sessions.read().await.len()
    }
}

impl Default for SessionTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::SessionRole;

    fn participant(peer_id: &str, role: SessionRole) -> SessionParticipant {
        SessionParticipant {
            peer_id: peer_id.to_string(),
            role,
        }
    }

    #[tokio::test]
    async fn update_session_creates_new_session() {
        let tracker = SessionTracker::new();
        tracker
            .update_session("s1", "tcp", 3, participant("peer-1", SessionRole::Entry))
            .await;

        let session = tracker.get_session("s1").await;
        assert!(session.is_some());
        let session = session.unwrap();
        assert_eq!(session.session_id, "s1");
        assert_eq!(session.protocol, "tcp");
        assert_eq!(session.hop_count, 3);
        assert_eq!(session.participants.len(), 1);
        assert_eq!(session.participants[0].peer_id, "peer-1");
        assert_eq!(session.participants[0].role, SessionRole::Entry);
    }

    #[tokio::test]
    async fn update_session_merges_participants() {
        let tracker = SessionTracker::new();
        tracker
            .update_session("s1", "tcp", 3, participant("peer-1", SessionRole::Entry))
            .await;
        tracker
            .update_session("s1", "tcp", 3, participant("peer-2", SessionRole::Relay))
            .await;

        let session = tracker.get_session("s1").await.unwrap();
        assert_eq!(session.participants.len(), 2);

        let peer_ids: Vec<&str> = session.participants.iter().map(|p| p.peer_id.as_str()).collect();
        assert!(peer_ids.contains(&"peer-1"));
        assert!(peer_ids.contains(&"peer-2"));
    }

    #[tokio::test]
    async fn update_session_updates_existing_participant_role() {
        let tracker = SessionTracker::new();
        tracker
            .update_session("s1", "tcp", 3, participant("peer-1", SessionRole::Entry))
            .await;
        tracker
            .update_session("s1", "tcp", 3, participant("peer-1", SessionRole::Exit))
            .await;

        let session = tracker.get_session("s1").await.unwrap();
        // Should still have one participant, not two
        assert_eq!(session.participants.len(), 1);
        assert_eq!(session.participants[0].role, SessionRole::Exit);
    }

    #[tokio::test]
    async fn list_sessions_sorted_by_last_seen_descending() {
        let tracker = SessionTracker::new();
        tracker
            .update_session("s1", "tcp", 1, participant("peer-1", SessionRole::Entry))
            .await;

        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        tracker
            .update_session("s2", "udp", 2, participant("peer-2", SessionRole::Entry))
            .await;

        let sessions = tracker.list_sessions().await;
        assert_eq!(sessions.len(), 2);
        // Most recently seen first
        assert_eq!(sessions[0].session_id, "s2");
        assert_eq!(sessions[1].session_id, "s1");
    }

    #[tokio::test]
    async fn get_session_returns_none_for_unknown() {
        let tracker = SessionTracker::new();
        assert!(tracker.get_session("nonexistent").await.is_none());
    }

    #[tokio::test]
    async fn get_session_peers_returns_participant_ids() {
        let tracker = SessionTracker::new();
        tracker
            .update_session("s1", "tcp", 3, participant("peer-1", SessionRole::Entry))
            .await;
        tracker
            .update_session("s1", "tcp", 3, participant("peer-2", SessionRole::Relay))
            .await;

        let peers = tracker.get_session_peers("s1").await;
        assert_eq!(peers.len(), 2);
        assert!(peers.contains(&"peer-1".to_string()));
        assert!(peers.contains(&"peer-2".to_string()));
    }

    #[tokio::test]
    async fn get_session_peers_returns_empty_for_unknown() {
        let tracker = SessionTracker::new();
        let peers = tracker.get_session_peers("nonexistent").await;
        assert!(peers.is_empty());
    }

    #[tokio::test]
    async fn session_count_reflects_tracked_sessions() {
        let tracker = SessionTracker::new();
        assert_eq!(tracker.session_count().await, 0);

        tracker
            .update_session("s1", "tcp", 3, participant("peer-1", SessionRole::Entry))
            .await;
        assert_eq!(tracker.session_count().await, 1);

        tracker
            .update_session("s2", "udp", 2, participant("peer-2", SessionRole::Entry))
            .await;
        assert_eq!(tracker.session_count().await, 2);

        // Updating existing session should not increase count
        tracker
            .update_session("s1", "tcp", 3, participant("peer-3", SessionRole::Exit))
            .await;
        assert_eq!(tracker.session_count().await, 2);
    }
}
