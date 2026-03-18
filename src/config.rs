use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use clap::Parser;
use serde::Deserialize;

/// HOPR Session Explorer — OTLP telemetry receiver with web UI.
#[derive(Parser, Debug)]
#[command(name = "hose", version, about)]
pub struct CliArgs {
    /// Path to TOML configuration file.
    #[arg(short, long, env = "HOSE_CONFIG")]
    pub config: Option<PathBuf>,

    /// Blokli GraphQL indexer endpoint URL.
    #[arg(long, env = "HOSE_INDEXER_ENDPOINT")]
    pub indexer_endpoint: Option<String>,

    /// Socket address for the gRPC OTLP receiver [default: 0.0.0.0:4317].
    #[arg(long, env = "HOSE_GRPC_LISTEN")]
    pub grpc_listen: Option<String>,

    /// Socket address for the HTTP web server [default: 0.0.0.0:8080].
    #[arg(long, env = "HOSE_HTTP_LISTEN")]
    pub http_listen: Option<String>,

    /// Path to the SQLite database file [default: hose.db].
    #[arg(long, env = "HOSE_DATABASE_PATH")]
    pub database_path: Option<String>,

    /// How long completed debug session data is retained, in hours [default: 24].
    #[arg(long, env = "HOSE_RETENTION_HOURS")]
    pub retention_hours: Option<u64>,

    /// Maximum number of telemetry records buffered before a batch flush [default: 1000].
    #[arg(long, env = "HOSE_WRITE_BUFFER_SIZE")]
    pub write_buffer_size: Option<usize>,

    /// Maximum seconds between batch flushes [default: 5].
    #[arg(long, env = "HOSE_WRITE_BUFFER_FLUSH_SECS")]
    pub write_buffer_flush_secs: Option<u64>,
}

/// TOML config file structure. All fields are optional so that a partial file
/// acts as a sparse overlay on top of built-in defaults.
#[derive(Deserialize, Debug, Default)]
pub struct FileConfig {
    pub indexer_endpoint: Option<String>,
    pub grpc_listen: Option<String>,
    pub http_listen: Option<String>,
    pub database_path: Option<String>,
    pub retention_hours: Option<u64>,
    pub write_buffer_size: Option<usize>,
    pub write_buffer_flush_secs: Option<u64>,
}

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
    /// Build a `Config` by merging CLI args (which include env var fallback via
    /// clap) on top of an optional TOML config file, on top of built-in defaults.
    ///
    /// Precedence (highest to lowest): CLI args > env vars > config file > defaults.
    fn from_cli_and_file(
        cli: CliArgs,
        file: FileConfig,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let defaults = Self::default();

        Ok(Self {
            indexer_endpoint: cli.indexer_endpoint.or(file.indexer_endpoint),
            grpc_listen_addr: cli
                .grpc_listen
                .or(file.grpc_listen)
                .and_then(|s| s.parse().ok())
                .unwrap_or(defaults.grpc_listen_addr),
            http_listen_addr: cli
                .http_listen
                .or(file.http_listen)
                .and_then(|s| s.parse().ok())
                .unwrap_or(defaults.http_listen_addr),
            database_path: cli
                .database_path
                .or(file.database_path)
                .map(PathBuf::from)
                .unwrap_or(defaults.database_path),
            retention_period: cli
                .retention_hours
                .or(file.retention_hours)
                .map(|h| Duration::from_secs(h * 3600))
                .unwrap_or(defaults.retention_period),
            write_buffer_size: cli
                .write_buffer_size
                .or(file.write_buffer_size)
                .unwrap_or(defaults.write_buffer_size),
            write_buffer_flush_interval: cli
                .write_buffer_flush_secs
                .or(file.write_buffer_flush_secs)
                .map(Duration::from_secs)
                .unwrap_or(defaults.write_buffer_flush_interval),
        })
    }

    /// Load configuration with 3-layer precedence:
    ///   config file  →  env vars  →  CLI parameters
    ///
    /// Each layer overrides values set by the previous one. All values fall
    /// back to built-in defaults when not specified at any layer.
    pub fn load() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let cli = CliArgs::parse();

        let file_config = if let Some(ref path) = cli.config {
            let contents = std::fs::read_to_string(path)?;
            toml::from_str::<FileConfig>(&contents)?
        } else {
            FileConfig::default()
        };

        Self::from_cli_and_file(cli, file_config)
    }

    /// Convenience alias that falls back to defaults on error.
    /// Kept for backward compatibility in tests and simple setups.
    pub fn from_env() -> Self {
        Self::load().unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sensible() {
        let config = Config::default();
        assert_eq!(config.grpc_listen_addr, "0.0.0.0:4317".parse().unwrap());
        assert_eq!(config.http_listen_addr, "0.0.0.0:8080".parse().unwrap());
        assert_eq!(config.database_path, PathBuf::from("hose.db"));
        assert_eq!(config.retention_period, Duration::from_secs(24 * 3600));
        assert_eq!(config.write_buffer_size, 1000);
        assert_eq!(config.write_buffer_flush_interval, Duration::from_secs(5));
        assert!(config.indexer_endpoint.is_none());
    }

    #[test]
    fn file_config_overrides_defaults() {
        let cli = CliArgs {
            config: None,
            indexer_endpoint: None,
            grpc_listen: None,
            http_listen: None,
            database_path: None,
            retention_hours: None,
            write_buffer_size: None,
            write_buffer_flush_secs: None,
        };

        let file = FileConfig {
            indexer_endpoint: Some("http://example.com/graphql".into()),
            grpc_listen: Some("127.0.0.1:9999".into()),
            http_listen: None,
            database_path: Some("/tmp/test.db".into()),
            retention_hours: Some(48),
            write_buffer_size: Some(500),
            write_buffer_flush_secs: Some(10),
        };

        let config = Config::from_cli_and_file(cli, file).unwrap();

        assert_eq!(
            config.indexer_endpoint.as_deref(),
            Some("http://example.com/graphql")
        );
        assert_eq!(config.grpc_listen_addr, "127.0.0.1:9999".parse().unwrap());
        // http_listen not in file → falls back to default
        assert_eq!(config.http_listen_addr, "0.0.0.0:8080".parse().unwrap());
        assert_eq!(config.database_path, PathBuf::from("/tmp/test.db"));
        assert_eq!(config.retention_period, Duration::from_secs(48 * 3600));
        assert_eq!(config.write_buffer_size, 500);
        assert_eq!(config.write_buffer_flush_interval, Duration::from_secs(10));
    }

    #[test]
    fn cli_overrides_file_config() {
        let cli = CliArgs {
            config: None,
            indexer_endpoint: Some("http://cli-endpoint.com".into()),
            grpc_listen: Some("127.0.0.1:1111".into()),
            http_listen: Some("127.0.0.1:2222".into()),
            database_path: Some("cli.db".into()),
            retention_hours: Some(12),
            write_buffer_size: Some(2000),
            write_buffer_flush_secs: Some(1),
        };

        let file = FileConfig {
            indexer_endpoint: Some("http://file-endpoint.com".into()),
            grpc_listen: Some("127.0.0.1:3333".into()),
            http_listen: Some("127.0.0.1:4444".into()),
            database_path: Some("file.db".into()),
            retention_hours: Some(72),
            write_buffer_size: Some(100),
            write_buffer_flush_secs: Some(30),
        };

        let config = Config::from_cli_and_file(cli, file).unwrap();

        // CLI values should win over file values
        assert_eq!(
            config.indexer_endpoint.as_deref(),
            Some("http://cli-endpoint.com")
        );
        assert_eq!(config.grpc_listen_addr, "127.0.0.1:1111".parse().unwrap());
        assert_eq!(config.http_listen_addr, "127.0.0.1:2222".parse().unwrap());
        assert_eq!(config.database_path, PathBuf::from("cli.db"));
        assert_eq!(config.retention_period, Duration::from_secs(12 * 3600));
        assert_eq!(config.write_buffer_size, 2000);
        assert_eq!(config.write_buffer_flush_interval, Duration::from_secs(1));
    }

    #[test]
    fn file_config_deserializes_from_toml() {
        let toml_str = r#"
            indexer_endpoint = "http://test.com/graphql"
            grpc_listen = "0.0.0.0:5555"
            retention_hours = 168
        "#;

        let file: FileConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(
            file.indexer_endpoint.as_deref(),
            Some("http://test.com/graphql")
        );
        assert_eq!(file.grpc_listen.as_deref(), Some("0.0.0.0:5555"));
        assert_eq!(file.retention_hours, Some(168));
        // Unset fields remain None
        assert!(file.http_listen.is_none());
        assert!(file.database_path.is_none());
        assert!(file.write_buffer_size.is_none());
        assert!(file.write_buffer_flush_secs.is_none());
    }
}
