use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A HOPRd node that has sent telemetry.
/// Tracked in-memory only — does not survive restarts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Peer {
    pub peer_id: String,
    pub last_seen: DateTime<Utc>,
}

/// Role of a peer within a HOPR session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionRole {
    Entry,
    Relay,
    Exit,
}

/// A participant in a HOPR session with its role.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionParticipant {
    pub peer_id: String,
    pub role: SessionRole,
}

/// An active HOPR session observed in OTLP telemetry.
/// Tracked in-memory only — does not survive restarts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HoprSession {
    pub session_id: String,
    pub protocol: String,
    pub hop_count: u32,
    pub participants: Vec<SessionParticipant>,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
}

/// Status of an operator-created debug session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DebugSessionStatus {
    Active,
    Completed,
}

/// An operator-created debug session — a time-bounded telemetry capture window.
/// Persisted in the embedded database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebugSession {
    pub id: Uuid,
    pub name: String,
    pub status: DebugSessionStatus,
    pub peer_ids: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
}

/// The type of a telemetry record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TelemetryType {
    Span,
    Metric,
    Log,
}

/// A telemetry record captured during an active debug session.
/// Persisted in the embedded database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryRecord {
    pub id: Uuid,
    pub debug_session_id: Uuid,
    pub peer_id: String,
    pub record_type: TelemetryType,
    pub timestamp: DateTime<Utc>,
    pub payload: serde_json::Value,
}

/// Routing decision for a peer's telemetry.
#[derive(Debug, Clone)]
pub enum RoutingDecision {
    /// No active debug sessions for this peer — drop telemetry.
    Discard,
    /// One or more debug sessions want this peer's telemetry.
    Retain { session_ids: Vec<Uuid> },
}
