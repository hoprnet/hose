use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
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
pub async fn create_session(
    State(state): State<AppState>,
    Json(req): Json<CreateDebugSessionRequest>,
) -> Result<(StatusCode, Json<DebugSession>), (StatusCode, String)> {
    let session = debug_sessions::create_debug_session(&state.db, &req.name, &req.peer_ids)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Register peers in the router for retention
    state.peer_router.add_session(session.id, &req.peer_ids).await;

    state.emit(Event::DebugSessionUpdated {
        session_id: session.id.to_string(),
    });

    Ok((StatusCode::CREATED, Json(session)))
}

/// GET /api/debug-sessions - List all debug sessions.
pub async fn list_sessions(
    State(state): State<AppState>,
) -> Result<Json<Vec<DebugSession>>, (StatusCode, String)> {
    let sessions = debug_sessions::list_debug_sessions(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(sessions))
}

/// GET /api/debug-sessions/:id - Get a specific debug session.
pub async fn get_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<DebugSession>, (StatusCode, String)> {
    let session_id: uuid::Uuid = id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "invalid session ID".to_string()))?;

    let session = debug_sessions::get_debug_session(&state.db, session_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "session not found".to_string()))?;

    Ok(Json(session))
}

/// POST /api/debug-sessions/:id/end - End a debug session.
pub async fn end_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let session_id: uuid::Uuid = id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "invalid session ID".to_string()))?;

    let ended = debug_sessions::end_debug_session(&state.db, session_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if !ended {
        return Err((StatusCode::NOT_FOUND, "session not found or already completed".to_string()));
    }

    // Remove from peer router
    state.peer_router.remove_session(session_id).await;

    state.emit(Event::DebugSessionUpdated {
        session_id: session_id.to_string(),
    });

    Ok(StatusCode::OK)
}
