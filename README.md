# HOSE - HOPR Session Explorer

HOSE is a debugging and diagnostic tool for the [HOPR](https://hoprnet.org)
network. It acts as a passive OTLP telemetry receiver with a web UI, enabling
operators to observe and analyze sessions spanning multiple nodes (entry, relay,
exit) without interfering with those nodes. Telemetry is selectively persisted
only when an operator creates a debug session, keeping storage cost at zero
during normal operation.

## Features

- **OTLP gRPC receiver** - accepts traces, metrics, and logs from an existing
  OpenTelemetry collector
- **Web dashboard** - live peer presence, active HOPR sessions, debug session
  management
- **Selective capture** - two-mode ingestion (discard/retain) controlled by
  operator-initiated debug sessions
- **Blockchain enrichment** - optional integration with the Blokli indexer for
  on-chain channel state (balances, closure status, ticket indices)
- **Live updates** - Server-Sent Events push peer, session, and telemetry
  notifications to the browser in real time
- **Embedded storage** - single-file SQLite database with configurable retention
- **Single binary** - no external dependencies beyond the database file

## Quick Start

### Local Development

```bash
# With Nix (recommended)
nix develop
hose-dev

# Or with Cargo directly
cargo run
```

The web UI is available at `http://localhost:8080` and the gRPC OTLP receiver
listens on `localhost:4317`.

### Docker

```bash
docker run -p 8080:8080 -p 4317:4317 -v hose-data:/data hopr/hose
```

### Kubernetes (Helm)

```bash
helm install hose charts/hose
```

See [charts/hose/README.md](charts/hose/README.md) for the full list of chart
values.

## Configuration

All settings can be provided via CLI flags, environment variables, or a TOML
config file. Precedence (highest to lowest): CLI flags > env vars > config
file > defaults.

| Env Var                        | CLI Flag                    | Default        | Description                                         |
| ------------------------------ | --------------------------- | -------------- | --------------------------------------------------- |
| `HOSE_CONFIG`                  | `--config`                  | -              | Path to TOML config file                            |
| `HOSE_GRPC_LISTEN`             | `--grpc-listen`             | `0.0.0.0:4317` | gRPC OTLP receiver bind address                     |
| `HOSE_HTTP_LISTEN`             | `--http-listen`             | `0.0.0.0:8080` | HTTP web server bind address                        |
| `HOSE_DATABASE_PATH`           | `--database-path`           | `hose.db`      | SQLite database file path                           |
| `HOSE_RETENTION_HOURS`         | `--retention-hours`         | `24`           | Hours to retain completed debug session data        |
| `HOSE_WRITE_BUFFER_SIZE`       | `--write-buffer-size`       | `1000`         | Max telemetry records buffered before batch flush   |
| `HOSE_WRITE_BUFFER_FLUSH_SECS` | `--write-buffer-flush-secs` | `5`            | Max seconds between batch flushes                   |
| `HOSE_INDEXER_ENDPOINT`        | `--indexer-endpoint`        | -              | Blokli GraphQL endpoint for channel data (optional) |
| `RUST_LOG`                     | -                           | `info`         | Log level filter                                    |

Example TOML config file:

```toml
grpc_listen = "0.0.0.0:4317"
http_listen = "0.0.0.0:8080"
database_path = "/data/hose.db"
retention_hours = 48
write_buffer_size = 2000
write_buffer_flush_secs = 3
indexer_endpoint = "http://blokli:8000/graphql"
```

## Architecture

HOSE is a single-process Rust service with two network-facing servers sharing
in-memory state:

- **gRPC server** (Tonic) - receives OTLP traces, metrics, and logs
- **HTTP server** (Axum) - serves the web UI, JSON API, and SSE event stream

Key internal components: peer tracker, session tracker, peer router
(retain/discard decision), batched write buffer, periodic cleanup task, and an
optional Blokli GraphQL client for blockchain channel data.

See [docs/design.md](docs/design.md) for the full design document.

## License

[MIT](LICENSE)
