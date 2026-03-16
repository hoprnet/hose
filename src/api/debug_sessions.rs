use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::Deserialize;

use crate::db::debug_sessions;
use crate::peer_router::PeerRouter;
use crate::server::{AppState, Event};
use crate::types::DebugSession;

#[derive(Debug, Deserialize)]
pub struct CreateDebugSessionRequest {
    pub name: String,
    pub peer_ids: Vec<String>,
}

/// POST /api/debug-sessions - Create a new debug session.
#[tracing::instrument(skip_all)]
pub async fn create_session(
    State(state): State<AppState>,
    Json(req): Json<CreateDebugSessionRequest>,
) -> Result<(StatusCode, Json<DebugSession>), (StatusCode, String)> {
    let session = debug_sessions::create_debug_session(&state.db, &req.name, &req.peer_ids)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to create debug session in database");
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        })?;

    // Register peers in the router for retention
    state
        .peer_router
        .add_session(session.id, &req.peer_ids)
        .await;

    state.emit(Event::DebugSessionUpdated {
        session_id: session.id.to_string(),
    });

    tracing::info!(
        session_id = %session.id,
        name = %session.name,
        peer_count = req.peer_ids.len(),
        "debug session created"
    );

    Ok((StatusCode::CREATED, Json(session)))
}

/// GET /api/debug-sessions - List all debug sessions.
#[tracing::instrument(skip(state))]
pub async fn list_sessions(
    State(state): State<AppState>,
) -> Result<Json<Vec<DebugSession>>, (StatusCode, String)> {
    let sessions = debug_sessions::list_debug_sessions(&state.db)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to list debug sessions");
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        })?;
    tracing::debug!(count = sessions.len(), "debug sessions listed");
    Ok(Json(sessions))
}

/// GET /api/debug-sessions/:id - Get a specific debug session.
#[tracing::instrument(skip(state))]
pub async fn get_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<DebugSession>, (StatusCode, String)> {
    let session_id: uuid::Uuid = id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "invalid session ID".to_string()))?;

    let session = debug_sessions::get_debug_session(&state.db, session_id)
        .await
        .map_err(|e| {
            tracing::error!(session_id = %session_id, error = %e, "failed to get debug session");
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        })?
        .ok_or_else(|| {
            tracing::warn!(session_id = %session_id, "debug session not found");
            (StatusCode::NOT_FOUND, "session not found".to_string())
        })?;

    Ok(Json(session))
}

/// POST /api/debug-sessions/:id/end - End a debug session.
#[tracing::instrument(skip(state))]
pub async fn end_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let session_id: uuid::Uuid = id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "invalid session ID".to_string()))?;

    let ended = debug_sessions::end_debug_session(&state.db, session_id)
        .await
        .map_err(|e| {
            tracing::error!(session_id = %session_id, error = %e, "failed to end debug session");
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        })?;

    if !ended {
        tracing::warn!(session_id = %session_id, "debug session not found or already completed");
        return Err((
            StatusCode::NOT_FOUND,
            "session not found or already completed".to_string(),
        ));
    }

    // Remove from peer router
    state.peer_router.remove_session(session_id).await;

    state.emit(Event::DebugSessionUpdated {
        session_id: session_id.to_string(),
    });

    tracing::info!(session_id = %session_id, "debug session ended");

    Ok(StatusCode::OK)
}
