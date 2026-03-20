use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::types::{DebugSession, DebugSessionStatus};

/// Create a new debug session with the given peers.
pub async fn create_debug_session(
    pool: &SqlitePool,
    name: &str,
    peer_ids: &[String],
) -> Result<DebugSession, sqlx::Error> {
    let id = Uuid::new_v4();
    let id_str = id.to_string();
    let now = Utc::now().to_rfc3339();

    let mut tx = pool.begin().await?;

    sqlx::query("INSERT INTO debug_sessions (id, name, status, created_at) VALUES (?, ?, 'active', ?)")
        .bind(&id_str)
        .bind(name)
        .bind(&now)
        .execute(&mut *tx)
        .await?;

    for peer_id in peer_ids {
        sqlx::query("INSERT INTO debug_session_peers (debug_session_id, peer_id) VALUES (?, ?)")
            .bind(&id_str)
            .bind(peer_id)
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;

    Ok(DebugSession {
        id,
        name: name.to_string(),
        status: DebugSessionStatus::Active,
        peer_ids: peer_ids.to_vec(),
        created_at: Utc::now(),
        ended_at: None,
    })
}

/// Get a debug session by ID, including its peer list.
pub async fn get_debug_session(pool: &SqlitePool, session_id: Uuid) -> Result<Option<DebugSession>, sqlx::Error> {
    let id_str = session_id.to_string();

    let row = sqlx::query_as::<_, (String, String, String, String, Option<String>)>(
        "SELECT id, name, status, created_at, ended_at FROM debug_sessions WHERE id = ?",
    )
    .bind(&id_str)
    .fetch_optional(pool)
    .await?;

    let Some((id, name, status, created_at, ended_at)) = row else {
        return Ok(None);
    };

    let peer_rows =
        sqlx::query_as::<_, (String,)>("SELECT peer_id FROM debug_session_peers WHERE debug_session_id = ?")
            .bind(&id_str)
            .fetch_all(pool)
            .await?;

    let peer_ids: Vec<String> = peer_rows.into_iter().map(|(p,)| p).collect();

    Ok(Some(DebugSession {
        id: id.parse().unwrap_or(session_id),
        name,
        status: match status.as_str() {
            "completed" => DebugSessionStatus::Completed,
            _ => DebugSessionStatus::Active,
        },
        peer_ids,
        created_at: created_at.parse().unwrap_or_else(|_| Utc::now()),
        ended_at: ended_at.and_then(|s| s.parse().ok()),
    }))
}

/// List all debug sessions, most recent first.
pub async fn list_debug_sessions(pool: &SqlitePool) -> Result<Vec<DebugSession>, sqlx::Error> {
    let rows = sqlx::query_as::<_, (String, String, String, String, Option<String>)>(
        "SELECT id, name, status, created_at, ended_at FROM debug_sessions ORDER BY created_at DESC",
    )
    .fetch_all(pool)
    .await?;

    let mut sessions = Vec::new();
    for (id, name, status, created_at, ended_at) in rows {
        let session_id: Uuid = id.parse().unwrap_or_else(|_| Uuid::new_v4());
        let peer_rows =
            sqlx::query_as::<_, (String,)>("SELECT peer_id FROM debug_session_peers WHERE debug_session_id = ?")
                .bind(&id)
                .fetch_all(pool)
                .await?;

        sessions.push(DebugSession {
            id: session_id,
            name,
            status: match status.as_str() {
                "completed" => DebugSessionStatus::Completed,
                _ => DebugSessionStatus::Active,
            },
            peer_ids: peer_rows.into_iter().map(|(p,)| p).collect(),
            created_at: created_at.parse().unwrap_or_else(|_| Utc::now()),
            ended_at: ended_at.and_then(|s| s.parse().ok()),
        });
    }

    Ok(sessions)
}

/// End a debug session by setting its status to completed.
pub async fn end_debug_session(pool: &SqlitePool, session_id: Uuid) -> Result<bool, sqlx::Error> {
    let now = Utc::now().to_rfc3339();
    let result =
        sqlx::query("UPDATE debug_sessions SET status = 'completed', ended_at = ? WHERE id = ? AND status = 'active'")
            .bind(&now)
            .bind(session_id.to_string())
            .execute(pool)
            .await?;

    Ok(result.rows_affected() > 0)
}

/// Delete expired completed sessions (older than retention period).
pub async fn delete_expired_sessions(pool: &SqlitePool, retention_hours: u64) -> Result<u64, sqlx::Error> {
    let cutoff = Utc::now() - chrono::Duration::hours(retention_hours as i64);
    let cutoff_str = cutoff.to_rfc3339();

    let result = sqlx::query("DELETE FROM debug_sessions WHERE status = 'completed' AND ended_at < ?")
        .bind(&cutoff_str)
        .execute(pool)
        .await?;

    Ok(result.rows_affected())
}
