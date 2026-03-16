-- Debug sessions: operator-created capture windows
CREATE TABLE IF NOT EXISTS debug_sessions (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'completed')),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    ended_at TEXT
);

-- Peers being monitored by a debug session (many-to-many)
CREATE TABLE IF NOT EXISTS debug_session_peers (
    debug_session_id TEXT NOT NULL REFERENCES debug_sessions(id) ON DELETE CASCADE,
    peer_id TEXT NOT NULL,
    PRIMARY KEY (debug_session_id, peer_id)
);

CREATE INDEX IF NOT EXISTS idx_debug_session_peers_peer
    ON debug_session_peers(peer_id);

-- Telemetry spans captured during debug sessions
CREATE TABLE IF NOT EXISTS telemetry_spans (
    id TEXT PRIMARY KEY NOT NULL,
    debug_session_id TEXT NOT NULL REFERENCES debug_sessions(id) ON DELETE CASCADE,
    peer_id TEXT NOT NULL,
    trace_id TEXT,
    span_id TEXT,
    parent_span_id TEXT,
    operation_name TEXT,
    start_time TEXT NOT NULL,
    end_time TEXT,
    status_code TEXT,
    attributes TEXT,
    resource_attributes TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_telemetry_spans_session
    ON telemetry_spans(debug_session_id);
CREATE INDEX IF NOT EXISTS idx_telemetry_spans_peer
    ON telemetry_spans(debug_session_id, peer_id);

-- Telemetry metrics captured during debug sessions
CREATE TABLE IF NOT EXISTS telemetry_metrics (
    id TEXT PRIMARY KEY NOT NULL,
    debug_session_id TEXT NOT NULL REFERENCES debug_sessions(id) ON DELETE CASCADE,
    peer_id TEXT NOT NULL,
    metric_name TEXT NOT NULL,
    metric_type TEXT,
    value REAL,
    unit TEXT,
    attributes TEXT,
    resource_attributes TEXT,
    timestamp TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_telemetry_metrics_session
    ON telemetry_metrics(debug_session_id);
CREATE INDEX IF NOT EXISTS idx_telemetry_metrics_peer
    ON telemetry_metrics(debug_session_id, peer_id);

-- Telemetry logs captured during debug sessions
CREATE TABLE IF NOT EXISTS telemetry_logs (
    id TEXT PRIMARY KEY NOT NULL,
    debug_session_id TEXT NOT NULL REFERENCES debug_sessions(id) ON DELETE CASCADE,
    peer_id TEXT NOT NULL,
    severity TEXT,
    body TEXT,
    attributes TEXT,
    resource_attributes TEXT,
    timestamp TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_telemetry_logs_session
    ON telemetry_logs(debug_session_id);
CREATE INDEX IF NOT EXISTS idx_telemetry_logs_peer
    ON telemetry_logs(debug_session_id, peer_id);
