use sqlx::SqlitePool;
use std::time::Duration;
use tokio::sync::mpsc;
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
            Err(mpsc::error::TrySendError::Full(_)) => {
                tracing::warn!("write buffer full, dropping telemetry record");
                false
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
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
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

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

async fn write_batch_to_db(
    pool: &SqlitePool,
    batch: &[WriteRecord],
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;

    for record in batch {
        let id = Uuid::new_v4().to_string();
        let session_id = record.debug_session_id.to_string();
        let timestamp = chrono::Utc::now().to_rfc3339();
        let payload_str = record.payload.to_string();

        match record.record_type {
            RecordType::Span => {
                let operation_name = record
                    .payload
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let trace_id = record
                    .payload
                    .get("traceId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let span_id = record
                    .payload
                    .get("spanId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                sqlx::query(
                    "INSERT INTO telemetry_spans (id, debug_session_id, peer_id, trace_id, span_id, operation_name, start_time, attributes) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
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
                let metric_name = record
                    .payload
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                sqlx::query(
                    "INSERT INTO telemetry_metrics (id, debug_session_id, peer_id, metric_name, timestamp, attributes) VALUES (?, ?, ?, ?, ?, ?)",
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
                let body = record
                    .payload
                    .get("body")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                sqlx::query(
                    "INSERT INTO telemetry_logs (id, debug_session_id, peer_id, severity, body, timestamp, attributes) VALUES (?, ?, ?, ?, ?, ?, ?)",
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
