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
