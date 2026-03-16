use sqlx::SqlitePool;
use uuid::Uuid;

/// Pagination parameters for telemetry queries.
#[derive(Debug, Clone)]
pub struct PaginationParams {
    pub limit: i64,
    pub offset: i64,
}

impl Default for PaginationParams {
    fn default() -> Self {
        Self {
            limit: 50,
            offset: 0,
        }
    }
}

/// A paginated result set.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PaginatedResult<T> {
    pub items: Vec<T>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
}

/// A span record from the database.
#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow)]
pub struct SpanRow {
    pub id: String,
    pub debug_session_id: String,
    pub peer_id: String,
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
    pub operation_name: Option<String>,
    pub start_time: String,
    pub end_time: Option<String>,
    pub attributes: Option<String>,
    pub created_at: String,
}

/// A metric record from the database.
#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow)]
pub struct MetricRow {
    pub id: String,
    pub debug_session_id: String,
    pub peer_id: String,
    pub metric_name: String,
    pub metric_type: Option<String>,
    pub value: Option<f64>,
    pub unit: Option<String>,
    pub attributes: Option<String>,
    pub timestamp: String,
    pub created_at: String,
}

/// A log record from the database.
#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow)]
pub struct LogRow {
    pub id: String,
    pub debug_session_id: String,
    pub peer_id: String,
    pub severity: Option<String>,
    pub body: Option<String>,
    pub attributes: Option<String>,
    pub timestamp: String,
    pub created_at: String,
}

/// Query spans for a debug session with pagination.
pub async fn query_spans(
    pool: &SqlitePool,
    session_id: Uuid,
    pagination: &PaginationParams,
) -> Result<PaginatedResult<SpanRow>, sqlx::Error> {
    let session_str = session_id.to_string();

    let total: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM telemetry_spans WHERE debug_session_id = ?")
            .bind(&session_str)
            .fetch_one(pool)
            .await?;

    let items = sqlx::query_as::<_, SpanRow>(
        "SELECT id, debug_session_id, peer_id, trace_id, span_id, operation_name, start_time, end_time, attributes, created_at FROM telemetry_spans WHERE debug_session_id = ? ORDER BY start_time DESC LIMIT ? OFFSET ?"
    )
    .bind(&session_str)
    .bind(pagination.limit)
    .bind(pagination.offset)
    .fetch_all(pool)
    .await?;

    Ok(PaginatedResult {
        items,
        total: total.0,
        limit: pagination.limit,
        offset: pagination.offset,
    })
}

/// Query metrics for a debug session with pagination.
pub async fn query_metrics(
    pool: &SqlitePool,
    session_id: Uuid,
    pagination: &PaginationParams,
) -> Result<PaginatedResult<MetricRow>, sqlx::Error> {
    let session_str = session_id.to_string();

    let total: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM telemetry_metrics WHERE debug_session_id = ?")
            .bind(&session_str)
            .fetch_one(pool)
            .await?;

    let items = sqlx::query_as::<_, MetricRow>(
        "SELECT id, debug_session_id, peer_id, metric_name, metric_type, value, unit, attributes, timestamp, created_at FROM telemetry_metrics WHERE debug_session_id = ? ORDER BY timestamp DESC LIMIT ? OFFSET ?"
    )
    .bind(&session_str)
    .bind(pagination.limit)
    .bind(pagination.offset)
    .fetch_all(pool)
    .await?;

    Ok(PaginatedResult {
        items,
        total: total.0,
        limit: pagination.limit,
        offset: pagination.offset,
    })
}

/// Query logs for a debug session with pagination.
pub async fn query_logs(
    pool: &SqlitePool,
    session_id: Uuid,
    pagination: &PaginationParams,
) -> Result<PaginatedResult<LogRow>, sqlx::Error> {
    let session_str = session_id.to_string();

    let total: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM telemetry_logs WHERE debug_session_id = ?")
            .bind(&session_str)
            .fetch_one(pool)
            .await?;

    let items = sqlx::query_as::<_, LogRow>(
        "SELECT id, debug_session_id, peer_id, severity, body, attributes, timestamp, created_at FROM telemetry_logs WHERE debug_session_id = ? ORDER BY timestamp DESC LIMIT ? OFFSET ?"
    )
    .bind(&session_str)
    .bind(pagination.limit)
    .bind(pagination.offset)
    .fetch_all(pool)
    .await?;

    Ok(PaginatedResult {
        items,
        total: total.0,
        limit: pagination.limit,
        offset: pagination.offset,
    })
}
