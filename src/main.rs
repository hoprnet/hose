use std::time::Duration;

use hose::blokli::BlokliClient;
use hose::cleanup::spawn_cleanup_task;
use hose::config::Config;
use hose::identity::IdentityBridge;
use hose::peer_router::PeerRouter;
use hose::peer_tracker::PeerTracker;
use hose::proto::logs_service::logs_service_server::LogsServiceServer;
use hose::proto::metrics_service::metrics_service_server::MetricsServiceServer;
use hose::proto::trace_service::trace_service_server::TraceServiceServer;
use hose::receiver::logs::LogsReceiver;
use hose::receiver::metrics::MetricsReceiver;
use hose::receiver::trace::TraceReceiver;
use hose::server::AppState;
use hose::session_tracker::SessionTracker;
use hose::write_buffer::spawn_write_buffer;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Initialize tracing with env-filter support (defaults to info level).
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    // Load configuration from environment variables.
    let config = Config::from_env();
    tracing::info!(?config, "loaded configuration");

    // Initialize the SQLite database pool and run migrations.
    let pool = hose::db::init_pool(&config).await?;

    // Create in-memory tracking structures.
    let peer_tracker = PeerTracker::new();
    let session_tracker = SessionTracker::new();
    let peer_router = PeerRouter::new();

    // Spawn the write buffer background task for batched telemetry persistence.
    let write_buffer = spawn_write_buffer(
        pool.clone(),
        config.write_buffer_size,
        config.write_buffer_flush_interval,
        100, // batch size
    );

    // Spawn the cleanup task to purge expired debug sessions.
    let retention_hours = config.retention_period.as_secs() / 3600;
    spawn_cleanup_task(pool.clone(), retention_hours);

    // Optionally create the Blokli client and spawn the channel watcher.
    let blokli_client = config.indexer_endpoint.as_ref().map(|endpoint| {
        let client = BlokliClient::new(endpoint.clone());
        tracing::info!(%endpoint, "blokli indexer client configured");
        client
    });

    if let Some(ref client) = blokli_client {
        let (change_tx, _) = tokio::sync::broadcast::channel(256);
        hose::blokli::subscriptions::spawn_channel_watcher(
            client.clone(),
            vec![],
            change_tx,
            Duration::from_secs(30),
        );
        tracing::info!("channel watcher spawned");
    }

    // Create the identity bridge for blockchain key <-> peer ID lookups.
    let _identity_bridge = IdentityBridge::new(blokli_client);

    // Build the shared application state (includes an SSE broadcast channel).
    let state = AppState::new(
        config.clone(),
        pool.clone(),
        peer_router.clone(),
        peer_tracker.clone(),
        session_tracker.clone(),
    );

    // Construct gRPC service receivers sharing the same tracking state.
    let event_tx = state.event_tx.clone();

    let trace_receiver = TraceReceiver {
        peer_tracker: peer_tracker.clone(),
        session_tracker: session_tracker.clone(),
        peer_router: peer_router.clone(),
        write_buffer: write_buffer.clone(),
        event_tx: event_tx.clone(),
    };

    let metrics_receiver = MetricsReceiver {
        peer_tracker: peer_tracker.clone(),
        peer_router: peer_router.clone(),
        write_buffer: write_buffer.clone(),
        event_tx: event_tx.clone(),
    };

    let logs_receiver = LogsReceiver {
        peer_tracker,
        peer_router,
        write_buffer,
        event_tx,
    };

    // Build the gRPC server.
    let grpc_addr = config.grpc_listen_addr;
    let grpc_server = tonic::transport::Server::builder()
        .add_service(TraceServiceServer::new(trace_receiver))
        .add_service(MetricsServiceServer::new(metrics_receiver))
        .add_service(LogsServiceServer::new(logs_receiver))
        .serve(grpc_addr);

    tracing::info!(%grpc_addr, "gRPC OTLP receiver listening");

    // Build the HTTP server.
    let http_addr = config.http_listen_addr;
    let router = hose::server::build_router(state);
    let http_listener = tokio::net::TcpListener::bind(http_addr).await?;
    tracing::info!(%http_addr, "HTTP server listening");

    let http_server = axum::serve(http_listener, router);

    // Run both servers concurrently, with graceful shutdown on ctrl-c.
    tokio::select! {
        result = grpc_server => {
            if let Err(e) = result {
                tracing::error!(error = %e, "gRPC server exited with error");
            }
        }
        result = http_server => {
            if let Err(e) = result {
                tracing::error!(error = %e, "HTTP server exited with error");
            }
        }
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("received ctrl-c, shutting down gracefully");
        }
    }

    // Allow in-flight requests to drain.
    pool.close().await;
    tracing::info!("shutdown complete");

    Ok(())
}
