use std::time::Duration;

use sqlx::SqlitePool;
use tokio::{
    sync::{mpsc, mpsc::error::TrySendError},
    time::MissedTickBehavior,
};
use uuid::Uuid;

/// A record queued for writing to the database.
#[derive(Debug, Clone)]
pub struct WriteRecord {
    pub debug_session_id: Uuid,
    pub peer_id: String,
    pub record_type: RecordType,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone)]
pub enum RecordType {
    Span,
    Metric,
    Log,
}

/// Handle for submitting records from gRPC handlers (non-blocking).
#[derive(Debug, Clone)]
pub struct WriteBufferSender {
    tx: mpsc::Sender<WriteRecord>,
}

impl WriteBufferSender {
    /// Attempt to enqueue a record. Returns false if the buffer is full (record dropped).
    pub fn try_send(&self, record: WriteRecord) -> bool {
        match self.tx.try_send(record) {
            Ok(()) => true,
            Err(TrySendError::Full(_)) => {
                tracing::warn!("write buffer full, dropping telemetry record");
                false
            }
            Err(TrySendError::Closed(_)) => {
                tracing::error!("write buffer closed");
                false
            }
        }
    }
}

/// Spawn the write buffer background task. Returns the sender handle.
pub fn spawn_write_buffer(
    pool: SqlitePool,
    buffer_size: usize,
    flush_interval: Duration,
    batch_size: usize,
) -> WriteBufferSender {
    let (tx, rx) = mpsc::channel(buffer_size);
    tokio::spawn(flush_loop(rx, pool, flush_interval, batch_size));
    WriteBufferSender { tx }
}

async fn flush_loop(
    mut rx: mpsc::Receiver<WriteRecord>,
    pool: SqlitePool,
    flush_interval: Duration,
    batch_size: usize,
) {
    let mut batch: Vec<WriteRecord> = Vec::with_capacity(batch_size);
    let mut interval = tokio::time::interval(flush_interval);
    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            Some(record) = rx.recv() => {
                batch.push(record);
                if batch.len() >= batch_size {
                    flush_batch(&pool, &mut batch).await;
                }
            }
            _ = interval.tick() => {
                if !batch.is_empty() {
                    flush_batch(&pool, &mut batch).await;
                }
            }
        }
    }
}

async fn flush_batch(pool: &SqlitePool, batch: &mut Vec<WriteRecord>) {
    let count = batch.len();

    if let Err(e) = write_batch_to_db(pool, batch).await {
        tracing::error!(error = %e, count, "failed to flush telemetry batch");
    } else {
        tracing::debug!(count, "flushed telemetry batch");
    }

    batch.clear();
}

async fn write_batch_to_db(pool: &SqlitePool, batch: &[WriteRecord]) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;

    for record in batch {
        let id = Uuid::new_v4().to_string();
        let session_id = record.debug_session_id.to_string();
        let timestamp = chrono::Utc::now().to_rfc3339();
        let payload_str = record.payload.to_string();

        match record.record_type {
            RecordType::Span => {
                let operation_name = record.payload.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let trace_id = record.payload.get("traceId").and_then(|v| v.as_str()).unwrap_or("");
                let span_id = record.payload.get("spanId").and_then(|v| v.as_str()).unwrap_or("");

                sqlx::query(
                    "INSERT INTO telemetry_spans (id, debug_session_id, peer_id, trace_id, span_id, operation_name, \
                     start_time, attributes) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(&id)
                .bind(&session_id)
                .bind(&record.peer_id)
                .bind(trace_id)
                .bind(span_id)
                .bind(operation_name)
                .bind(&timestamp)
                .bind(&payload_str)
                .execute(&mut *tx)
                .await?;
            }
            RecordType::Metric => {
                let metric_name = record.payload.get("name").and_then(|v| v.as_str()).unwrap_or("");

                sqlx::query(
                    "INSERT INTO telemetry_metrics (id, debug_session_id, peer_id, metric_name, timestamp, \
                     attributes) VALUES (?, ?, ?, ?, ?, ?)",
                )
                .bind(&id)
                .bind(&session_id)
                .bind(&record.peer_id)
                .bind(metric_name)
                .bind(&timestamp)
                .bind(&payload_str)
                .execute(&mut *tx)
                .await?;
            }
            RecordType::Log => {
                let severity = record
                    .payload
                    .get("severityText")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let body = record.payload.get("body").and_then(|v| v.as_str()).unwrap_or("");

                sqlx::query(
                    "INSERT INTO telemetry_logs (id, debug_session_id, peer_id, severity, body, timestamp, \
                     attributes) VALUES (?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(&id)
                .bind(&session_id)
                .bind(&record.peer_id)
                .bind(severity)
                .bind(body)
                .bind(&timestamp)
                .bind(&payload_str)
                .execute(&mut *tx)
                .await?;
            }
        }
    }

    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use serde_json::json;
    use sqlx::SqlitePool;

    use super::*;

    /// Create an in-memory SQLite pool with the schema applied.
    async fn setup_pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::raw_sql(include_str!("../migrations/001_initial_schema.sql"))
            .execute(&pool)
            .await
            .unwrap();
        pool
    }

    /// Insert a debug session and return its id.
    async fn insert_debug_session(pool: &SqlitePool) -> Uuid {
        let session_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO debug_sessions (id, name, status, created_at) VALUES (?, 'test', 'active', datetime('now'))",
        )
        .bind(session_id.to_string())
        .execute(pool)
        .await
        .unwrap();
        session_id
    }

    fn make_record(session_id: Uuid, record_type: RecordType) -> WriteRecord {
        WriteRecord {
            debug_session_id: session_id,
            peer_id: "peer-1".to_string(),
            record_type,
            payload: json!({"name": "test-span", "traceId": "trace-1", "spanId": "span-1"}),
        }
    }

    #[tokio::test]
    async fn try_send_returns_true_when_buffer_has_space() {
        let pool = setup_pool().await;
        let session_id = insert_debug_session(&pool).await;

        let sender = spawn_write_buffer(pool, 16, Duration::from_secs(60), 100);
        let result = sender.try_send(make_record(session_id, RecordType::Span));

        assert!(result, "try_send should return true when buffer has space");
    }

    #[tokio::test]
    async fn try_send_returns_false_when_buffer_is_full() {
        // Create a channel with capacity 1 but no consumer, so the buffer stays full.
        let (tx, _rx) = mpsc::channel::<WriteRecord>(1);
        let sender = WriteBufferSender { tx };
        let session_id = Uuid::new_v4();

        // First send fills the single-slot buffer.
        let first = sender.try_send(make_record(session_id, RecordType::Span));
        assert!(first, "first try_send should succeed");

        // Second send should fail because nobody is consuming and buffer_size is 1.
        let second = sender.try_send(make_record(session_id, RecordType::Span));
        assert!(!second, "try_send should return false when buffer is full");
    }

    #[tokio::test]
    async fn records_flushed_to_database_after_flush_interval() {
        let pool = setup_pool().await;
        let session_id = insert_debug_session(&pool).await;

        let sender = spawn_write_buffer(
            pool.clone(),
            64,
            Duration::from_millis(50),
            1000, // large batch_size so only the timer triggers flush
        );

        sender.try_send(make_record(session_id, RecordType::Span));

        // Wait long enough for the flush interval to fire.
        tokio::time::sleep(Duration::from_millis(200)).await;

        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM telemetry_spans")
            .fetch_one(&pool)
            .await
            .unwrap();

        assert_eq!(count.0, 1, "one span record should have been flushed to the database");
    }

    #[tokio::test]
    async fn batch_flush_when_batch_size_reached() {
        let pool = setup_pool().await;
        let session_id = insert_debug_session(&pool).await;

        let batch_size = 3;
        let sender = spawn_write_buffer(
            pool.clone(),
            64,
            Duration::from_secs(600), // very long interval so only batch_size triggers flush
            batch_size,
        );

        for _ in 0..batch_size {
            let sent = sender.try_send(make_record(session_id, RecordType::Span));
            assert!(sent, "should be able to send within buffer capacity");
        }

        // Give the background task time to receive all records and flush.
        tokio::time::sleep(Duration::from_millis(200)).await;

        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM telemetry_spans")
            .fetch_one(&pool)
            .await
            .unwrap();

        assert_eq!(
            count.0, batch_size as i64,
            "all records should be flushed once batch_size is reached"
        );
    }

    #[tokio::test]
    async fn try_send_returns_false_when_channel_closed() {
        // Manually create a channel and drop the receiver to simulate a closed channel.
        let (tx, rx) = mpsc::channel::<WriteRecord>(8);
        drop(rx);

        let sender = WriteBufferSender { tx };
        let session_id = Uuid::new_v4();
        let result = sender.try_send(make_record(session_id, RecordType::Log));

        assert!(!result, "try_send should return false when the channel is closed");
    }
}
