use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use hose::{
    blokli::{BlokliClient, subscriptions::spawn_channel_watcher},
    cleanup::spawn_cleanup_task,
    config::Config,
    identity::IdentityBridge,
    peer_router::PeerRouter,
    peer_tracker::PeerTracker,
    proto::{
        logs_service::logs_service_server::LogsServiceServer,
        metrics_service::metrics_service_server::MetricsServiceServer,
        trace_service::trace_service_server::TraceServiceServer,
    },
    receiver::{logs::LogsReceiver, metrics::MetricsReceiver, trace::TraceReceiver},
    server::AppState,
    session_tracker::SessionTracker,
    write_buffer::spawn_write_buffer,
};
use tokio::{net::TcpListener, sync::broadcast};
use tonic::transport::{Server, server::TcpIncoming};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Initialize tracing with env-filter support (defaults to info level).
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    // Load configuration from config file, environment variables, and CLI parameters.
    let config = Config::load()?;
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
        let (change_tx, _) = broadcast::channel(256);
        spawn_channel_watcher(client.clone(), vec![], change_tx, Duration::from_secs(30));
        tracing::info!("channel watcher spawned");
    }

    // Create the identity bridge for blockchain key <-> peer ID lookups.
    let identity_bridge = IdentityBridge::new(blokli_client.clone());

    // Shared flag set to true once the gRPC listener binds successfully.
    let grpc_ready = Arc::new(AtomicBool::new(false));

    // Build the shared application state (includes an SSE broadcast channel).
    let (event_tx, _) = broadcast::channel(1024);
    let state = AppState {
        config: Arc::new(config.clone()),
        db: pool.clone(),
        peer_router: peer_router.clone(),
        peer_tracker: peer_tracker.clone(),
        session_tracker: session_tracker.clone(),
        identity_bridge,
        blokli_client,
        event_tx,
        grpc_ready: grpc_ready.clone(),
    };

    // Construct gRPC service receivers sharing the same tracking state.
    let event_tx = state.event_tx.clone();

    let trace_receiver = TraceReceiver {
        peer_tracker: peer_tracker.clone(),
        session_tracker: session_tracker.clone(),
        peer_router: peer_router.clone(),
        write_buffer: write_buffer.clone(),
        event_tx: event_tx.clone(),
        last_trace_sample: Arc::new(Mutex::new(None)),
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

    // Build the gRPC server. Bind the listener first so we can signal readiness.
    let grpc_addr = config.grpc_listen_addr;
    let grpc_listener = TcpListener::bind(grpc_addr).await?;
    tracing::info!(%grpc_addr, "gRPC OTLP receiver listening");
    grpc_ready.store(true, Ordering::Relaxed);

    let grpc_incoming = TcpIncoming::from_listener(grpc_listener, true, None)?;
    let grpc_server = Server::builder()
        .add_service(TraceServiceServer::new(trace_receiver))
        .add_service(MetricsServiceServer::new(metrics_receiver))
        .add_service(LogsServiceServer::new(logs_receiver))
        .serve_with_incoming(grpc_incoming);

    // Build the HTTP server.
    let http_addr = config.http_listen_addr;
    let router = hose::server::build_router(state);
    let http_listener = TcpListener::bind(http_addr).await?;
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
