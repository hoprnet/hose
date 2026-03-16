use hose::peer_router::PeerRouter;
use hose::peer_tracker::PeerTracker;
use hose::proto::common::{AnyValue, KeyValue, any_value};
use hose::proto::resource::Resource;
use hose::proto::trace::{ResourceSpans, ScopeSpans, Span};
use hose::proto::trace_service::ExportTraceServiceRequest;
use hose::proto::trace_service::trace_service_server::TraceService;
use hose::receiver::trace::TraceReceiver;
use hose::session_tracker::SessionTracker;
use hose::write_buffer::spawn_write_buffer;

use sqlx::SqlitePool;
use std::time::Duration;
use tokio::sync::broadcast;
use tonic::Request;
use uuid::Uuid;

/// Build a KeyValue with a string value.
fn string_kv(key: &str, value: &str) -> KeyValue {
    KeyValue {
        key: key.to_string(),
        value: Some(AnyValue {
            value: Some(any_value::Value::StringValue(value.to_string())),
        }),
    }
}

/// Build a KeyValue with an integer value.
fn int_kv(key: &str, value: i64) -> KeyValue {
    KeyValue {
        key: key.to_string(),
        value: Some(AnyValue {
            value: Some(any_value::Value::IntValue(value)),
        }),
    }
}

/// Build a minimal ExportTraceServiceRequest with one span for a given peer.
fn trace_request_for_peer(peer_id: &str, span_name: &str) -> ExportTraceServiceRequest {
    ExportTraceServiceRequest {
        resource_spans: vec![ResourceSpans {
            resource: Some(Resource {
                attributes: vec![string_kv("service.instance.id", peer_id)],
                dropped_attributes_count: 0,
            }),
            scope_spans: vec![ScopeSpans {
                scope: None,
                spans: vec![Span {
                    trace_id: vec![1; 16],
                    span_id: vec![2; 8],
                    parent_span_id: vec![],
                    name: span_name.to_string(),
                    kind: 0,
                    start_time_unix_nano: 1_000_000_000,
                    end_time_unix_nano: 2_000_000_000,
                    attributes: vec![],
                    dropped_attributes_count: 0,
                    events: vec![],
                    dropped_events_count: 0,
                    links: vec![],
                    dropped_links_count: 0,
                    status: None,
                    flags: 0,
                    trace_state: String::new(),
                }],
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }],
    }
}

/// Build a trace request that includes HOPR session attributes on the span.
fn trace_request_with_session(
    peer_id: &str,
    session_id: &str,
    protocol: &str,
    hops: i64,
    role: &str,
) -> ExportTraceServiceRequest {
    ExportTraceServiceRequest {
        resource_spans: vec![ResourceSpans {
            resource: Some(Resource {
                attributes: vec![string_kv("hopr.peer_id", peer_id)],
                dropped_attributes_count: 0,
            }),
            scope_spans: vec![ScopeSpans {
                scope: None,
                spans: vec![Span {
                    trace_id: vec![3; 16],
                    span_id: vec![4; 8],
                    parent_span_id: vec![],
                    name: "hopr.session.relay".to_string(),
                    kind: 0,
                    start_time_unix_nano: 1_000_000_000,
                    end_time_unix_nano: 2_000_000_000,
                    attributes: vec![
                        string_kv("hopr.session.id", session_id),
                        string_kv("hopr.session.protocol", protocol),
                        int_kv("hopr.session.hops", hops),
                        string_kv("hopr.session.role", role),
                    ],
                    dropped_attributes_count: 0,
                    events: vec![],
                    dropped_events_count: 0,
                    links: vec![],
                    dropped_links_count: 0,
                    status: None,
                    flags: 0,
                    trace_state: String::new(),
                }],
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }],
    }
}

/// Create a TraceReceiver backed by an in-memory SQLite database.
async fn make_receiver() -> (TraceReceiver, SqlitePool) {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();

    // Enable foreign keys for in-memory SQLite
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&pool)
        .await
        .unwrap();

    // Run the schema migration (may contain multiple statements)
    let schema = include_str!("../migrations/001_initial_schema.sql");
    for statement in schema.split(';') {
        let trimmed = statement.trim();
        if !trimmed.is_empty() {
            sqlx::query(trimmed).execute(&pool).await.unwrap();
        }
    }

    let peer_tracker = PeerTracker::new();
    let session_tracker = SessionTracker::new();
    let peer_router = PeerRouter::new();
    let write_buffer = spawn_write_buffer(pool.clone(), 1024, Duration::from_millis(50), 64);
    let (event_tx, _) = broadcast::channel(128);

    let receiver = TraceReceiver {
        peer_tracker,
        session_tracker,
        peer_router,
        write_buffer,
        event_tx,
    };

    (receiver, pool)
}

// ---------------------------------------------------------------------------
// Test 1: Peer tracking through trace ingestion
// ---------------------------------------------------------------------------

#[tokio::test]
async fn trace_export_registers_peer_in_tracker() {
    let (receiver, _pool) = make_receiver().await;

    let peer_id = "16Uiu2HAmPeer1";
    let req = Request::new(trace_request_for_peer(peer_id, "test.span"));

    let resp = receiver.export(req).await;
    assert!(resp.is_ok(), "export should succeed");

    // Peer should now be tracked
    assert!(
        receiver.peer_tracker.is_tracked(peer_id).await,
        "peer should be tracked after receiving trace data"
    );
    assert_eq!(receiver.peer_tracker.peer_count().await, 1);

    let peer = receiver.peer_tracker.get_peer(peer_id).await;
    assert!(peer.is_some(), "get_peer should return the peer");
    assert_eq!(peer.unwrap().peer_id, peer_id);
}

#[tokio::test]
async fn trace_export_with_hopr_peer_id_attribute_registers_peer() {
    let (receiver, _pool) = make_receiver().await;

    let peer_id = "16Uiu2HAmPeer2";
    // Use "hopr.peer_id" attribute instead of "service.instance.id"
    let req = Request::new(trace_request_with_session(
        peer_id, "ignored", "tcp", 1, "entry",
    ));

    let resp = receiver.export(req).await;
    assert!(resp.is_ok());

    assert!(
        receiver.peer_tracker.is_tracked(peer_id).await,
        "peer should be tracked when using hopr.peer_id attribute"
    );
}

#[tokio::test]
async fn trace_export_with_empty_peer_id_is_ignored() {
    let (receiver, _pool) = make_receiver().await;

    let req = Request::new(trace_request_for_peer("", "test.span"));

    let resp = receiver.export(req).await;
    assert!(
        resp.is_ok(),
        "export should succeed even with empty peer_id"
    );
    assert_eq!(
        receiver.peer_tracker.peer_count().await,
        0,
        "no peer should be tracked for empty peer_id"
    );
}

// ---------------------------------------------------------------------------
// Test 2: Session tracking through trace ingestion
// ---------------------------------------------------------------------------

#[tokio::test]
async fn trace_export_with_session_attributes_updates_session_tracker() {
    let (receiver, _pool) = make_receiver().await;

    let peer_id = "16Uiu2HAmSessionPeer";
    let session_id = "sess-abc-123";
    let req = Request::new(trace_request_with_session(
        peer_id, session_id, "tcp", 3, "entry",
    ));

    let resp = receiver.export(req).await;
    assert!(resp.is_ok());

    // Session should now be tracked
    assert_eq!(receiver.session_tracker.session_count().await, 1);

    let session = receiver.session_tracker.get_session(session_id).await;
    assert!(session.is_some(), "session should exist in tracker");

    let session = session.unwrap();
    assert_eq!(session.session_id, session_id);
    assert_eq!(session.protocol, "tcp");
    assert_eq!(session.hop_count, 3);
    assert_eq!(session.participants.len(), 1);
    assert_eq!(session.participants[0].peer_id, peer_id);
    assert_eq!(
        session.participants[0].role,
        hose::types::SessionRole::Entry
    );
}

#[tokio::test]
async fn trace_export_without_session_attributes_does_not_create_session() {
    let (receiver, _pool) = make_receiver().await;

    let peer_id = "16Uiu2HAmNoSession";
    let req = Request::new(trace_request_for_peer(peer_id, "regular.span"));

    let resp = receiver.export(req).await;
    assert!(resp.is_ok());

    // Peer is tracked, but no session should appear
    assert!(receiver.peer_tracker.is_tracked(peer_id).await);
    assert_eq!(
        receiver.session_tracker.session_count().await,
        0,
        "no session should be created when span has no session attributes"
    );
}

// ---------------------------------------------------------------------------
// Test 3: Retained telemetry is written to DB through WriteBuffer
// ---------------------------------------------------------------------------

#[tokio::test]
async fn retained_trace_is_written_to_database() {
    let (receiver, pool) = make_receiver().await;

    let peer_id = "16Uiu2HAmRetained";
    let debug_session_id = Uuid::new_v4();

    // Register a debug session that retains this peer's telemetry
    receiver
        .peer_router
        .add_session(debug_session_id, &[peer_id.to_string()])
        .await;

    // Also insert the debug_session row so the FK constraint is satisfied
    sqlx::query("INSERT INTO debug_sessions (id, name, status) VALUES (?, ?, 'active')")
        .bind(debug_session_id.to_string())
        .bind("test-session")
        .execute(&pool)
        .await
        .unwrap();

    let req = Request::new(trace_request_for_peer(peer_id, "retained.span"));
    let resp = receiver.export(req).await;
    assert!(resp.is_ok());

    // Allow the write buffer flush loop to run
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Verify the span was written to telemetry_spans
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM telemetry_spans")
        .fetch_one(&pool)
        .await
        .unwrap();

    assert!(
        count.0 > 0,
        "at least one span should be written to DB when peer is retained"
    );

    // Verify the written record references the correct debug session and peer
    let row: (String, String) =
        sqlx::query_as("SELECT debug_session_id, peer_id FROM telemetry_spans LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();

    assert_eq!(row.0, debug_session_id.to_string());
    assert_eq!(row.1, peer_id);
}

// ---------------------------------------------------------------------------
// Test 4: Discarded telemetry produces no DB writes
// ---------------------------------------------------------------------------

#[tokio::test]
async fn discarded_trace_produces_no_db_writes() {
    let (receiver, pool) = make_receiver().await;

    let peer_id = "16Uiu2HAmDiscarded";

    // No debug session registered for this peer -> routing decision is Discard
    let req = Request::new(trace_request_for_peer(peer_id, "discarded.span"));
    let resp = receiver.export(req).await;
    assert!(resp.is_ok());

    // Allow the write buffer flush loop some time to demonstrate nothing is written
    tokio::time::sleep(Duration::from_millis(200)).await;

    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM telemetry_spans")
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_eq!(
        count.0, 0,
        "no spans should be written to DB when peer has no active debug session"
    );

    let metric_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM telemetry_metrics")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(metric_count.0, 0);

    let log_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM telemetry_logs")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(log_count.0, 0);
}

// ---------------------------------------------------------------------------
// Additional edge-case tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn multiple_peers_tracked_independently() {
    let (receiver, _pool) = make_receiver().await;

    let peers = ["16Uiu2HAmAlpha", "16Uiu2HAmBravo", "16Uiu2HAmCharlie"];
    for peer_id in &peers {
        let req = Request::new(trace_request_for_peer(peer_id, "multi.span"));
        receiver.export(req).await.unwrap();
    }

    assert_eq!(receiver.peer_tracker.peer_count().await, 3);
    for peer_id in &peers {
        assert!(receiver.peer_tracker.is_tracked(peer_id).await);
    }
}

#[tokio::test]
async fn retained_span_includes_correct_operation_name() {
    let (receiver, pool) = make_receiver().await;

    let peer_id = "16Uiu2HAmOpName";
    let debug_session_id = Uuid::new_v4();

    receiver
        .peer_router
        .add_session(debug_session_id, &[peer_id.to_string()])
        .await;

    sqlx::query("INSERT INTO debug_sessions (id, name, status) VALUES (?, ?, 'active')")
        .bind(debug_session_id.to_string())
        .bind("op-name-test")
        .execute(&pool)
        .await
        .unwrap();

    let req = Request::new(trace_request_for_peer(peer_id, "hopr.relay.forward"));
    receiver.export(req).await.unwrap();

    tokio::time::sleep(Duration::from_millis(200)).await;

    let row: (String,) = sqlx::query_as("SELECT operation_name FROM telemetry_spans LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_eq!(row.0, "hopr.relay.forward");
}

#[tokio::test]
async fn export_returns_success_response() {
    let (receiver, _pool) = make_receiver().await;

    let req = Request::new(trace_request_for_peer("16Uiu2HAmResp", "resp.span"));
    let resp = receiver.export(req).await.unwrap();

    // The response should have no partial_success (all data accepted)
    assert!(resp.into_inner().partial_success.is_none());
}
