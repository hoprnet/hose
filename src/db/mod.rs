pub mod debug_sessions;
pub mod telemetry;

use std::{str::FromStr, time::Duration};

use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
};

use crate::config::Config;

/// Initialize the SQLite database: create the pool and run migrations.
pub async fn init_pool(config: &Config) -> Result<SqlitePool, sqlx::Error> {
    let db_url = format!("sqlite:{}?mode=rwc", config.database_path.display());

    let options = SqliteConnectOptions::from_str(&db_url)?
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5));

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;

    // Enable WAL checkpoint on close
    sqlx::query("PRAGMA wal_autocheckpoint = 1000").execute(&pool).await?;

    run_migrations(&pool).await?;

    tracing::info!(
        path = %config.database_path.display(),
        "database initialized"
    );

    Ok(pool)
}

/// Run database migrations.
async fn run_migrations(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query(include_str!("../../migrations/001_initial_schema.sql"))
        .execute(pool)
        .await?;
    Ok(())
}
