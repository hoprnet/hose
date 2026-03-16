use sqlx::SqlitePool;
use std::time::Duration;

use crate::db::debug_sessions;

/// Spawn a background task that periodically purges expired completed debug sessions.
/// Runs every hour and deletes sessions older than the retention period.
/// The SQLite schema uses CASCADE DELETE so associated telemetry data is automatically removed.
pub fn spawn_cleanup_task(pool: SqlitePool, retention_hours: u64) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(3600));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            interval.tick().await;

            match debug_sessions::delete_expired_sessions(&pool, retention_hours).await {
                Ok(count) => {
                    if count > 0 {
                        tracing::info!(count, "cleaned up expired debug sessions");
                    } else {
                        tracing::debug!("cleanup: no expired sessions to remove");
                    }
                }
                Err(e) => {
                    tracing::error!(error = %e, "failed to clean up expired sessions");
                }
            }
        }
    });
}
