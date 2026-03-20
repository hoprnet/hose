use hose::{
    config::Config,
    identity::IdentityBridge,
    peer_router::PeerRouter,
    peer_tracker::PeerTracker,
    server::{AppState, build_router},
    session_tracker::SessionTracker,
    types::{DebugSession, Peer},
};
use reqwest::Client;
use sqlx::SqlitePool;
use tokio::net::TcpListener;

/// Set up an in-memory SQLite database with the schema applied.
async fn setup_db() -> SqlitePool {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();

    let schema = include_str!("../migrations/001_initial_schema.sql");
    for statement in schema.split(';') {
        let trimmed = statement.trim();
        if !trimmed.is_empty() {
            sqlx::query(trimmed).execute(&pool).await.unwrap();
        }
    }

    sqlx::query("PRAGMA foreign_keys = ON").execute(&pool).await.unwrap();

    pool
}

/// Spawn the Axum HTTP server on a random port and return the base URL and shared state.
async fn spawn_test_server() -> (String, AppState) {
    let config = Config::default();
    let pool = setup_db().await;

    let peer_tracker = PeerTracker::new();
    let session_tracker = SessionTracker::new();
    let peer_router = PeerRouter::new();
    let identity_bridge = IdentityBridge::new(None);

    let state = AppState::new(
        config,
        pool,
        peer_router,
        peer_tracker,
        session_tracker,
        identity_bridge,
        None,
    );

    let router = build_router(state.clone());
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    let base_url = format!("http://{}", addr);
    (base_url, state)
}

// ---------------------------------------------------------------------------
// Test 1: Health check endpoint returns "ok"
// ---------------------------------------------------------------------------

#[tokio::test]
async fn health_check_returns_ok() {
    let (base_url, _state) = spawn_test_server().await;
    let client = Client::new();

    let resp = client.get(format!("{}/health", base_url)).send().await.unwrap();

    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.unwrap(), "ok");
}

// ---------------------------------------------------------------------------
// Test 2: Create a debug session via POST /api/debug-sessions
// ---------------------------------------------------------------------------

#[tokio::test]
async fn create_debug_session_returns_201() {
    let (base_url, _state) = spawn_test_server().await;
    let client = Client::new();

    let resp = client
        .post(format!("{}/api/debug-sessions", base_url))
        .json(&serde_json::json!({
            "name": "test session",
            "peer_ids": ["peer1", "peer2"]
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 201, "expected 201 Created");

    let session: DebugSession = resp.json().await.unwrap();
    assert_eq!(session.name, "test session");
    assert_eq!(session.peer_ids.len(), 2);
    assert!(session.peer_ids.contains(&"peer1".to_string()));
    assert!(session.peer_ids.contains(&"peer2".to_string()));
    assert_eq!(
        session.status,
        hose::types::DebugSessionStatus::Active,
        "session should start as active"
    );
    assert!(session.ended_at.is_none(), "ended_at should be None for a new session");
}

// ---------------------------------------------------------------------------
// Test 3: List debug sessions via GET /api/debug-sessions
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_debug_sessions_contains_created_session() {
    let (base_url, _state) = spawn_test_server().await;
    let client = Client::new();

    // Create a session first
    let create_resp = client
        .post(format!("{}/api/debug-sessions", base_url))
        .json(&serde_json::json!({
            "name": "listed session",
            "peer_ids": ["peer-a"]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(create_resp.status(), 201);
    let created: DebugSession = create_resp.json().await.unwrap();

    // List sessions
    let list_resp = client
        .get(format!("{}/api/debug-sessions", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(list_resp.status(), 200);

    let sessions: Vec<DebugSession> = list_resp.json().await.unwrap();
    assert!(!sessions.is_empty(), "session list should not be empty after creation");

    let found = sessions.iter().find(|s| s.id == created.id);
    assert!(found.is_some(), "the created session should appear in the list");
    assert_eq!(found.unwrap().name, "listed session");
}

// ---------------------------------------------------------------------------
// Test 4: Get a specific debug session via GET /api/debug-sessions/:id
// ---------------------------------------------------------------------------

#[tokio::test]
async fn get_debug_session_by_id() {
    let (base_url, _state) = spawn_test_server().await;
    let client = Client::new();

    // Create a session
    let create_resp = client
        .post(format!("{}/api/debug-sessions", base_url))
        .json(&serde_json::json!({
            "name": "detail session",
            "peer_ids": ["node-1", "node-2", "node-3"]
        }))
        .send()
        .await
        .unwrap();
    let created: DebugSession = create_resp.json().await.unwrap();

    // Fetch it by ID
    let get_resp = client
        .get(format!("{}/api/debug-sessions/{}", base_url, created.id))
        .send()
        .await
        .unwrap();
    assert_eq!(get_resp.status(), 200);

    let session: DebugSession = get_resp.json().await.unwrap();
    assert_eq!(session.id, created.id);
    assert_eq!(session.name, "detail session");
    assert_eq!(session.peer_ids.len(), 3);
}

#[tokio::test]
async fn get_nonexistent_debug_session_returns_404() {
    let (base_url, _state) = spawn_test_server().await;
    let client = Client::new();

    let fake_id = uuid::Uuid::new_v4();
    let resp = client
        .get(format!("{}/api/debug-sessions/{}", base_url, fake_id))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 404);
}

// ---------------------------------------------------------------------------
// Test 5: End a debug session via POST /api/debug-sessions/:id/end
// ---------------------------------------------------------------------------

#[tokio::test]
async fn end_debug_session_transitions_to_completed() {
    let (base_url, _state) = spawn_test_server().await;
    let client = Client::new();

    // Create a session
    let create_resp = client
        .post(format!("{}/api/debug-sessions", base_url))
        .json(&serde_json::json!({
            "name": "session to end",
            "peer_ids": ["peer-x"]
        }))
        .send()
        .await
        .unwrap();
    let created: DebugSession = create_resp.json().await.unwrap();

    // End it
    let end_resp = client
        .post(format!("{}/api/debug-sessions/{}/end", base_url, created.id))
        .send()
        .await
        .unwrap();
    assert_eq!(end_resp.status(), 200);

    // Verify the session is now completed
    let get_resp = client
        .get(format!("{}/api/debug-sessions/{}", base_url, created.id))
        .send()
        .await
        .unwrap();
    let session: DebugSession = get_resp.json().await.unwrap();

    assert_eq!(
        session.status,
        hose::types::DebugSessionStatus::Completed,
        "session should be completed after ending"
    );
    assert!(session.ended_at.is_some(), "ended_at should be set after ending");
}

#[tokio::test]
async fn end_already_completed_session_returns_404() {
    let (base_url, _state) = spawn_test_server().await;
    let client = Client::new();

    // Create and end a session
    let create_resp = client
        .post(format!("{}/api/debug-sessions", base_url))
        .json(&serde_json::json!({
            "name": "double end",
            "peer_ids": ["peer-y"]
        }))
        .send()
        .await
        .unwrap();
    let created: DebugSession = create_resp.json().await.unwrap();

    client
        .post(format!("{}/api/debug-sessions/{}/end", base_url, created.id))
        .send()
        .await
        .unwrap();

    // Try ending it again
    let second_end = client
        .post(format!("{}/api/debug-sessions/{}/end", base_url, created.id))
        .send()
        .await
        .unwrap();

    assert_eq!(
        second_end.status(),
        404,
        "ending an already-completed session should return 404"
    );
}

// ---------------------------------------------------------------------------
// Test: Trace inspector page returns 200
// ---------------------------------------------------------------------------

#[tokio::test]
async fn inspector_page_returns_200() {
    let (base_url, _state) = spawn_test_server().await;
    let client = Client::new();

    let resp = client.get(format!("{}/inspector", base_url)).send().await.unwrap();

    assert_eq!(resp.status(), 200, "inspector page should return 200");
    let body = resp.text().await.unwrap();
    assert!(
        body.contains("Trace Inspector"),
        "inspector page should contain the title"
    );
}

// ---------------------------------------------------------------------------
// Test: SSE events endpoint returns correct content type
// ---------------------------------------------------------------------------

#[tokio::test]
async fn events_endpoint_returns_sse_content_type() {
    let (base_url, _state) = spawn_test_server().await;
    let client = Client::new();

    let resp = client.get(format!("{}/api/events", base_url)).send().await.unwrap();

    assert_eq!(resp.status(), 200);
    let content_type = resp
        .headers()
        .get("content-type")
        .expect("should have content-type header")
        .to_str()
        .unwrap();
    assert!(
        content_type.contains("text/event-stream"),
        "SSE endpoint should return text/event-stream, got: {content_type}"
    );
}

// ---------------------------------------------------------------------------
// Test: Static file serving returns CSS
// ---------------------------------------------------------------------------

#[tokio::test]
async fn static_css_returns_200() {
    let (base_url, _state) = spawn_test_server().await;
    let client = Client::new();

    let resp = client
        .get(format!("{}/static/css/hose.css", base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200, "static CSS file should return 200");
    let body = resp.text().await.unwrap();
    assert!(
        body.contains("HOSE Design System"),
        "CSS file should contain design system comment"
    );
}

// ---------------------------------------------------------------------------
// Test 6: GET /api/peers returns empty list initially
// ---------------------------------------------------------------------------

#[tokio::test]
async fn peers_returns_empty_list_initially() {
    let (base_url, _state) = spawn_test_server().await;
    let client = Client::new();

    let resp = client.get(format!("{}/api/peers", base_url)).send().await.unwrap();

    assert_eq!(resp.status(), 200);

    let peers: Vec<Peer> = resp.json().await.unwrap();
    assert!(peers.is_empty(), "peer list should be empty initially");
}

// ---------------------------------------------------------------------------
// Test 7: After registering a peer via PeerTracker, GET /api/peers returns it
// ---------------------------------------------------------------------------

#[tokio::test]
async fn peers_returns_tracked_peer() {
    let (base_url, state) = spawn_test_server().await;
    let client = Client::new();

    // Simulate a peer being seen (as the gRPC receiver would do)
    state.peer_tracker.record_seen("16Uiu2HAmTestPeer").await;

    let resp = client.get(format!("{}/api/peers", base_url)).send().await.unwrap();

    assert_eq!(resp.status(), 200);

    let peers: Vec<Peer> = resp.json().await.unwrap();
    assert_eq!(peers.len(), 1, "should have exactly one peer");
    assert_eq!(peers[0].peer_id, "16Uiu2HAmTestPeer");
}

#[tokio::test]
async fn peers_returns_multiple_tracked_peers_sorted() {
    let (base_url, state) = spawn_test_server().await;
    let client = Client::new();

    state.peer_tracker.record_seen("charlie").await;
    state.peer_tracker.record_seen("alice").await;
    state.peer_tracker.record_seen("bob").await;

    let resp = client.get(format!("{}/api/peers", base_url)).send().await.unwrap();

    let peers: Vec<Peer> = resp.json().await.unwrap();
    assert_eq!(peers.len(), 3);

    let ids: Vec<&str> = peers.iter().map(|p| p.peer_id.as_str()).collect();
    assert_eq!(ids, vec!["alice", "bob", "charlie"]);
}

// ---------------------------------------------------------------------------
// Test: Full lifecycle - create, list, get, end debug session
// ---------------------------------------------------------------------------

#[tokio::test]
async fn debug_session_full_lifecycle() {
    let (base_url, _state) = spawn_test_server().await;
    let client = Client::new();

    // Step 1: List sessions - should be empty
    let list_resp = client
        .get(format!("{}/api/debug-sessions", base_url))
        .send()
        .await
        .unwrap();
    let sessions: Vec<DebugSession> = list_resp.json().await.unwrap();
    assert!(sessions.is_empty(), "should start with no sessions");

    // Step 2: Create a session
    let create_resp = client
        .post(format!("{}/api/debug-sessions", base_url))
        .json(&serde_json::json!({
            "name": "lifecycle test",
            "peer_ids": ["peer-1", "peer-2"]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(create_resp.status(), 201);
    let created: DebugSession = create_resp.json().await.unwrap();

    // Step 3: List sessions - should have one
    let list_resp = client
        .get(format!("{}/api/debug-sessions", base_url))
        .send()
        .await
        .unwrap();
    let sessions: Vec<DebugSession> = list_resp.json().await.unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].id, created.id);

    // Step 4: Get by ID
    let get_resp = client
        .get(format!("{}/api/debug-sessions/{}", base_url, created.id))
        .send()
        .await
        .unwrap();
    assert_eq!(get_resp.status(), 200);
    let fetched: DebugSession = get_resp.json().await.unwrap();
    assert_eq!(fetched.status, hose::types::DebugSessionStatus::Active);

    // Step 5: End the session
    let end_resp = client
        .post(format!("{}/api/debug-sessions/{}/end", base_url, created.id))
        .send()
        .await
        .unwrap();
    assert_eq!(end_resp.status(), 200);

    // Step 6: Verify it is completed
    let get_resp = client
        .get(format!("{}/api/debug-sessions/{}", base_url, created.id))
        .send()
        .await
        .unwrap();
    let ended: DebugSession = get_resp.json().await.unwrap();
    assert_eq!(ended.status, hose::types::DebugSessionStatus::Completed);
    assert!(ended.ended_at.is_some());
}
