use std::time::Duration;

use hose::{
    db::debug_sessions::{
        create_debug_session, delete_expired_sessions, end_debug_session, get_debug_session, list_debug_sessions,
    },
    types::DebugSessionStatus,
};
use sqlx::SqlitePool;
use uuid::Uuid;

/// Set up an in-memory SQLite database with the schema applied.
async fn setup_db() -> SqlitePool {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::query(include_str!("../migrations/001_initial_schema.sql"))
        .execute(&pool)
        .await
        .unwrap();
    // Enable foreign keys for cascade deletes
    sqlx::query("PRAGMA foreign_keys = ON").execute(&pool).await.unwrap();
    pool
}

#[tokio::test]
async fn create_session_stores_with_correct_fields() {
    let pool = setup_db().await;
    let peer_ids = vec!["peer-a".to_string(), "peer-b".to_string()];

    let session = create_debug_session(&pool, "test-session", &peer_ids).await.unwrap();

    assert_eq!(session.name, "test-session");
    assert_eq!(session.status, DebugSessionStatus::Active);
    assert_eq!(session.peer_ids, peer_ids);
    assert!(session.ended_at.is_none());

    // Verify it was persisted by reading it back
    let fetched = get_debug_session(&pool, session.id).await.unwrap().unwrap();
    assert_eq!(fetched.name, "test-session");
    assert_eq!(fetched.status, DebugSessionStatus::Active);
    assert_eq!(fetched.peer_ids.len(), 2);
    assert!(fetched.peer_ids.contains(&"peer-a".to_string()));
    assert!(fetched.peer_ids.contains(&"peer-b".to_string()));
}

#[tokio::test]
async fn list_sessions_returns_most_recent_first() {
    let pool = setup_db().await;

    let first = create_debug_session(&pool, "first", &["p1".to_string()]).await.unwrap();

    // Insert a small delay so created_at timestamps differ
    tokio::time::sleep(Duration::from_millis(50)).await;

    let second = create_debug_session(&pool, "second", &["p2".to_string()])
        .await
        .unwrap();

    let sessions = list_debug_sessions(&pool).await.unwrap();
    assert_eq!(sessions.len(), 2);
    // Most recent first
    assert_eq!(sessions[0].id, second.id);
    assert_eq!(sessions[1].id, first.id);
}

#[tokio::test]
async fn get_session_by_id_returns_all_fields() {
    let pool = setup_db().await;
    let peer_ids = vec!["node-1".to_string(), "node-2".to_string(), "node-3".to_string()];

    let created = create_debug_session(&pool, "full-fields", &peer_ids).await.unwrap();

    let fetched = get_debug_session(&pool, created.id).await.unwrap().unwrap();

    assert_eq!(fetched.id, created.id);
    assert_eq!(fetched.name, "full-fields");
    assert_eq!(fetched.status, DebugSessionStatus::Active);
    assert_eq!(fetched.peer_ids.len(), 3);
    assert!(fetched.peer_ids.contains(&"node-1".to_string()));
    assert!(fetched.peer_ids.contains(&"node-2".to_string()));
    assert!(fetched.peer_ids.contains(&"node-3".to_string()));
    assert!(fetched.ended_at.is_none());
}

#[tokio::test]
async fn end_session_changes_status_and_sets_ended_at() {
    let pool = setup_db().await;

    let session = create_debug_session(&pool, "to-end", &["p1".to_string()])
        .await
        .unwrap();

    let ended = end_debug_session(&pool, session.id).await.unwrap();
    assert!(ended, "end_debug_session should return true for an active session");

    let fetched = get_debug_session(&pool, session.id).await.unwrap().unwrap();
    assert_eq!(fetched.status, DebugSessionStatus::Completed);
    assert!(
        fetched.ended_at.is_some(),
        "ended_at should be set after ending a session"
    );
}

#[tokio::test]
async fn end_already_completed_session_returns_false() {
    let pool = setup_db().await;

    let session = create_debug_session(&pool, "double-end", &["p1".to_string()])
        .await
        .unwrap();

    let first_end = end_debug_session(&pool, session.id).await.unwrap();
    assert!(first_end);

    let second_end = end_debug_session(&pool, session.id).await.unwrap();
    assert!(!second_end, "ending an already-completed session should return false");
}

#[tokio::test]
async fn get_nonexistent_session_returns_none() {
    let pool = setup_db().await;

    let result = get_debug_session(&pool, Uuid::new_v4()).await.unwrap();
    assert!(result.is_none(), "getting a non-existent session should return None");
}

#[tokio::test]
async fn delete_expired_sessions_cleans_up_completed_sessions() {
    let pool = setup_db().await;

    let session = create_debug_session(&pool, "expiring", &["p1".to_string()])
        .await
        .unwrap();

    // End the session so it becomes completed
    end_debug_session(&pool, session.id).await.unwrap();

    // Manually backdate the ended_at to simulate an old session
    let old_time = (chrono::Utc::now() - chrono::Duration::hours(48)).to_rfc3339();
    sqlx::query("UPDATE debug_sessions SET ended_at = ? WHERE id = ?")
        .bind(&old_time)
        .bind(session.id.to_string())
        .execute(&pool)
        .await
        .unwrap();

    // Delete sessions with a 24-hour retention; our session ended 48 hours ago
    let deleted = delete_expired_sessions(&pool, 24).await.unwrap();
    assert_eq!(deleted, 1, "should delete one expired session");

    // Verify the session is gone
    let fetched = get_debug_session(&pool, session.id).await.unwrap();
    assert!(fetched.is_none(), "expired session should be deleted from the database");
}
