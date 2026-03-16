use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

/// Application configuration.
#[derive(Debug, Clone)]
pub struct Config {
    /// Blokli GraphQL indexer endpoint URL.
    pub indexer_endpoint: Option<String>,

    /// Socket address for the gRPC OTLP receiver.
    pub grpc_listen_addr: SocketAddr,

    /// Socket address for the HTTP web server.
    pub http_listen_addr: SocketAddr,

    /// Path to the SQLite database file.
    pub database_path: PathBuf,

    /// How long completed debug session data is retained before cleanup.
    pub retention_period: Duration,

    /// Maximum number of telemetry records buffered before a batch flush.
    pub write_buffer_size: usize,

    /// Maximum time between batch flushes.
    pub write_buffer_flush_interval: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            indexer_endpoint: None,
            grpc_listen_addr: "0.0.0.0:4317".parse().unwrap(),
            http_listen_addr: "0.0.0.0:8080".parse().unwrap(),
            database_path: PathBuf::from("hose.db"),
            retention_period: Duration::from_secs(24 * 60 * 60),
            write_buffer_size: 1000,
            write_buffer_flush_interval: Duration::from_secs(5),
        }
    }
}

impl Config {
    /// Load configuration from environment variables, falling back to defaults.
    pub fn from_env() -> Self {
        let default = Self::default();

        Self {
            indexer_endpoint: std::env::var("HOSE_INDEXER_ENDPOINT").ok(),
            grpc_listen_addr: std::env::var("HOSE_GRPC_LISTEN")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(default.grpc_listen_addr),
            http_listen_addr: std::env::var("HOSE_HTTP_LISTEN")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(default.http_listen_addr),
            database_path: std::env::var("HOSE_DATABASE_PATH")
                .ok()
                .map(PathBuf::from)
                .unwrap_or(default.database_path),
            retention_period: std::env::var("HOSE_RETENTION_HOURS")
                .ok()
                .and_then(|s| s.parse::<u64>().ok())
                .map(|h| Duration::from_secs(h * 3600))
                .unwrap_or(default.retention_period),
            write_buffer_size: std::env::var("HOSE_WRITE_BUFFER_SIZE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(default.write_buffer_size),
            write_buffer_flush_interval: std::env::var("HOSE_WRITE_BUFFER_FLUSH_SECS")
                .ok()
                .and_then(|s| s.parse::<u64>().ok())
                .map(Duration::from_secs)
                .unwrap_or(default.write_buffer_flush_interval),
        }
    }
}
