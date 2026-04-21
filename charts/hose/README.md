# HOSE Helm Chart

Deploys [HOSE](../../README.md) (HOPR Session Explorer) on Kubernetes -- an OTLP
telemetry receiver with a web UI for debugging HOPR network sessions.

## Prerequisites

- Kubernetes 1.24+
- Helm 3+

## Installation

```bash
helm install hose charts/hose
```

Override values inline or with a file:

```bash
helm install hose charts/hose \
  --set config.indexerEndpoint="http://blokli:8000/graphql" \
  --set ingress.enabled=true
```

## Values

### Image

| Key                | Type   | Default        | Description                                 |
| ------------------ | ------ | -------------- | ------------------------------------------- |
| `replicaCount`     | int    | `1`            | Must be 1 (SQLite single-writer constraint) |
| `image.repository` | string | `hopr/hose`    | Container image repository                  |
| `image.tag`        | string | `""`           | Image tag (defaults to Chart.AppVersion)    |
| `image.pullPolicy` | string | `IfNotPresent` | Image pull policy                           |
| `imagePullSecrets` | list   | `[]`           | Registry pull secrets                       |
| `nameOverride`     | string | `""`           | Override chart name                         |
| `fullnameOverride` | string | `""`           | Override fully qualified name               |

### Service Account

| Key                          | Type   | Default | Description                    |
| ---------------------------- | ------ | ------- | ------------------------------ |
| `serviceAccount.create`      | bool   | `true`  | Create a ServiceAccount        |
| `serviceAccount.annotations` | object | `{}`    | ServiceAccount annotations     |
| `serviceAccount.name`        | string | `""`    | Name (auto-generated if empty) |

### Networking

| Key                   | Type   | Default         | Description                              |
| --------------------- | ------ | --------------- | ---------------------------------------- |
| `service.type`        | string | `ClusterIP`     | Service type for web                     |
| `service.grpcType`    | string | `LoadBalancer`  | Service type for ingestor                |
| `ingress.enabled`     | bool   | `false`         | Enable Ingress for the web UI            |
| `ingress.className`   | string | `""`            | Ingress class name                       |
| `ingress.annotations` | object | `{}`            | Ingress annotations                      |
| `ingress.hosts`       | list   | see values.yaml | Ingress host rules                       |
| `ingress.tls`         | list   | `[]`            | TLS configuration                        |

### Persistence

| Key                        | Type   | Default           | Description                             |
| -------------------------- | ------ | ----------------- | --------------------------------------- |
| `persistence.enabled`      | bool   | `true`            | Enable PVC for SQLite data              |
| `persistence.size`         | string | `1Gi`             | PVC size                                |
| `persistence.storageClass` | string | `""`              | StorageClass (cluster default if empty) |
| `persistence.accessModes`  | list   | `[ReadWriteOnce]` | PVC access modes                        |

HOSE stores debug session data in an embedded SQLite database. Because SQLite
does not support concurrent writers, `replicaCount` must stay at 1. The PVC is
mounted at `/data` inside the container.

### Application Configuration

These values map directly to HOSE environment variables via a ConfigMap.

| Key                           | Env Var                        | Default         | Description                              |
| ----------------------------- | ------------------------------ | --------------- | ---------------------------------------- |
| `config.grpcListen`           | `HOSE_GRPC_LISTEN`             | `0.0.0.0:4317`  | gRPC OTLP receiver bind address          |
| `config.httpListen`           | `HOSE_HTTP_LISTEN`             | `0.0.0.0:8080`  | HTTP web server bind address             |
| `config.databasePath`         | `HOSE_DATABASE_PATH`           | `/data/hose.db` | SQLite database file path                |
| `config.retentionHours`       | `HOSE_RETENTION_HOURS`         | `24`            | Hours to retain completed debug sessions |
| `config.writeBufferSize`      | `HOSE_WRITE_BUFFER_SIZE`       | `1000`          | Max telemetry records before batch flush |
| `config.writeBufferFlushSecs` | `HOSE_WRITE_BUFFER_FLUSH_SECS` | `5`             | Max seconds between batch flushes        |
| `config.indexerEndpoint`      | `HOSE_INDEXER_ENDPOINT`        | `""`            | Blokli GraphQL endpoint (optional)       |
| `config.rustLog`              | `RUST_LOG`                     | `info`          | Rust log level filter                    |

### Extra Environment

| Key            | Type | Default | Description                               |
| -------------- | ---- | ------- | ----------------------------------------- |
| `extraEnv`     | list | `[]`    | Additional env vars for the container     |
| `extraEnvFrom` | list | `[]`    | Additional envFrom sources (e.g. Secrets) |

### Security

| Key                  | Type   | Default | Description                      |
| -------------------- | ------ | ------- | -------------------------------- |
| `podSecurityContext` | object | `{}`    | Pod-level security context       |
| `securityContext`    | object | `{}`    | Container-level security context |

### Probes

| Key              | Type   | Default                     | Description                   |
| ---------------- | ------ | --------------------------- | ----------------------------- |
| `readinessProbe` | object | HTTP GET `/` on port `http` | Readiness probe configuration |
| `livenessProbe`  | object | TCP socket on port `http`   | Liveness probe configuration  |

### Resources and Scheduling

| Key              | Type   | Default | Description                    |
| ---------------- | ------ | ------- | ------------------------------ |
| `resources`      | object | `{}`    | CPU/memory requests and limits |
| `nodeSelector`   | object | `{}`    | Node selector constraints      |
| `tolerations`    | list   | `[]`    | Tolerations                    |
| `affinity`       | object | `{}`    | Affinity rules                 |
| `podAnnotations` | object | `{}`    | Additional pod annotations     |
| `podLabels`      | object | `{}`    | Additional pod labels          |

## Sending Telemetry

Configure your OTLP collector to forward telemetry to the gRPC endpoint:

```yaml
# In your OpenTelemetry Collector config
exporters:
  otlp/hose:
    endpoint: "hose.default.svc.cluster.local:4317"
    tls:
      insecure: true
```

Replace `hose.default` with your release name and namespace.
