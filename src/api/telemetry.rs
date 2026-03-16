use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::Deserialize;

use crate::db::telemetry::{self, LogRow, MetricRow, PaginatedResult, PaginationParams, SpanRow};
use crate::server::AppState;

#[derive(Debug, Deserialize)]
pub struct PaginationQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

impl From<PaginationQuery> for PaginationParams {
    fn from(q: PaginationQuery) -> Self {
        PaginationParams {
            limit: q.limit.unwrap_or(50),
            offset: q.offset.unwrap_or(0),
        }
    }
}

/// GET /api/debug-sessions/:id/spans - Paginated spans for a debug session.
pub async fn query_spans(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(pagination): Query<PaginationQuery>,
) -> Result<Json<PaginatedResult<SpanRow>>, (StatusCode, String)> {
    let session_id: uuid::Uuid = id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "invalid session ID".to_string()))?;

    let result = telemetry::query_spans(&state.db, session_id, &pagination.into())
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(result))
}

/// GET /api/debug-sessions/:id/metrics - Paginated metrics for a debug session.
pub async fn query_metrics(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(pagination): Query<PaginationQuery>,
) -> Result<Json<PaginatedResult<MetricRow>>, (StatusCode, String)> {
    let session_id: uuid::Uuid = id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "invalid session ID".to_string()))?;

    let result = telemetry::query_metrics(&state.db, session_id, &pagination.into())
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(result))
}

/// GET /api/debug-sessions/:id/logs - Paginated logs for a debug session.
pub async fn query_logs(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(pagination): Query<PaginationQuery>,
) -> Result<Json<PaginatedResult<LogRow>>, (StatusCode, String)> {
    let session_id: uuid::Uuid = id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "invalid session ID".to_string()))?;

    let result = telemetry::query_logs(&state.db, session_id, &pagination.into())
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(result))
}
