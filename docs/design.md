# HOSE — High-Level Design

## Table of Contents

- [1. Purpose](#1-purpose)
  - [1.1 Problem it solves](#11-problem-it-solves)
- [2. System Overview](#2-system-overview)
- [3. Core Concepts](#3-core-concepts)
  - [3.1 Peer Tracking](#31-peer-tracking)
  - [3.2 HOPR Session Tracking](#32-hopr-session-tracking)
  - [3.3 Two-Mode Ingestion (Discard / Retain)](#33-two-mode-ingestion-discard--retain)
  - [3.4 Debug Sessions](#34-debug-sessions)
  - [3.5 Batched Write Buffer](#35-batched-write-buffer)
  - [3.6 Data Enrichment from the Blockchain Layer](#36-data-enrichment-from-the-blockchain-layer)
- [4. Data Flow](#4-data-flow)
  - [4.1 Idle Operation (Discard Mode)](#41-idle-operation-discard-mode)
  - [4.2 Active Debugging (Retain Mode)](#42-active-debugging-retain-mode)
  - [4.3 Session Teardown and Cleanup](#43-session-teardown-and-cleanup)
- [5. External Integration: Blokli Blockchain Indexer](#5-external-integration-blokli-blockchain-indexer)
  - [5.1 What Blokli Provides](#51-what-blokli-provides)
  - [5.2 Why This Matters for Debugging](#52-why-this-matters-for-debugging)
  - [5.3 Identity Bridging](#53-identity-bridging)
- [6. Web Interface](#6-web-interface)
  - [6.1 HTML Pages](#61-html-pages)
  - [6.2 JSON API](#62-json-api)
  - [6.3 Live Event Stream](#63-live-event-stream)
- [7. Design Decisions & Trade-offs](#7-design-decisions--trade-offs)
- [8. Operational Characteristics](#8-operational-characteristics)
  - [8.1 Deployment](#81-deployment)
  - [8.2 Configuration Surface](#82-configuration-surface)
  - [8.3 Data Retention](#83-data-retention)
  - [8.4 Error Handling Philosophy](#84-error-handling-philosophy)
  - [8.5 Concurrency Model](#85-concurrency-model)

---

## 1. Purpose

**HOSE** (HOPR Session Explorer) is a debugging and diagnostic tool for the HOPR
network. It enables operators to observe and analyze sessions spanning multiple
nodes (entry, relay, and exit) without actively interfering with those nodes.

HOSE exists because debugging a HOPR session today requires manually querying
each node's REST API, piecing together transient state across multiple
terminals, and losing all data when the tools close. HOSE replaces this workflow
by acting as a **passive sink** for the telemetry that HOPRd nodes already emit,
providing a single point of observation across the entire session path, while
enriching that view with on-chain channel data from the Blokli blockchain
indexer.

### 1.1 Problem it solves

- **Cross-node visibility**: A HOPR session involves an entry node, zero or more
  relay nodes, and an exit node. Understanding session behavior requires
  correlating telemetry from all of them simultaneously.
- **Transient event capture**: REST API polling only sees current state.
  Telemetry captures events as they happen — session establishment steps,
  packet-level timing, intermediate relay behavior, error conditions.
- **Collaborative debugging**: Multiple operators can view the same data
  concurrently through a shared web interface, rather than each running their
  own isolated tool.
- **Temporal persistence**: Debug data survives page refreshes and can be
  reviewed after the fact, within a configurable retention window.

---

## 2. System Overview

HOSE is a single-process service with two network-facing servers:

1. **gRPC Receiver** — Accepts OpenTelemetry (OTLP) telemetry data (traces,
   metrics, logs) forwarded from an existing OTLP collector in the
   infrastructure.
2. **Web Server** — Serves an HTML interface for operators, a JSON API for
   programmatic access, and a live event stream for real-time updates.

Both servers share in-process state. Telemetry is selectively persisted to an
embedded relational database (SQLite) only when an operator explicitly requests
it.

The service is deployed as a **single binary** with no external dependencies
beyond the database file it creates on disk.

The following diagram shows the component layout within the HOSE process and its
relationship to the external telemetry pipeline:

```
              HOPRd Nodes
    ┌──────────┐  ┌──────────┐  ┌──────────┐
    │  Entry   │  │  Relay   │  │   Exit   │
    └────┬─────┘  └────┬─────┘  └────┬─────┘
         │             │             │
         └──────┬──────┴─────────────┘
                │  OTLP telemetry
                ▼
         ┌─────────────────────┐
         │   OTLP Collector    │
         │   (infrastructure)  │
         └────────┬────────────┘
                  │  gRPC forward
                  ▼
┌────────────────────────────────────────────────┐
│                    HOSE                        │
│                                                │
│  ┌─────────────────┐   ┌────────────────────┐  │
│  │  gRPC Receiver  │   │    Web Server      │  │
│  │                 │   │                    │  │
│  │  Trace intake   │   │  HTML pages        │  │
│  │  Metric intake  │   │  JSON API          │  │
│  │  Log intake     │   │  Live event stream │  │
│  └────────┬────────┘   └─────────┬──────────┘  │
│           │                      │             │
│           ▼                      ▼             │
│  ┌──────────────────────────────────────────┐  │
│  │           Application Core               │  │
│  │                                          │  │
│  │  ┌──────────────┐  ┌─────────────────┐   │  │
│  │  │ Peer Tracker │  │ Session Tracker │   │  │
│  │  │ (presence)   │  │ (HOPR sessions) │   │  │
│  │  └──────────────┘  └─────────────────┘   │  │
│  │                                          │  │
│  │  ┌──────────────┐  ┌─────────────────┐   │  │
│  │  │ Peer Router  │  │ Write Buffer    │   │  │
│  │  │ (retain/     │  │ (batched        │   │  │
│  │  │  discard)    │  │  writes)        │   │  │
│  │  └──────────────┘  └─────────────────┘   │  │
│  │                                          │  │
│  │  ┌──────────────┐  ┌─────────────────┐   │  │
│  │  │ Cleanup Task │  │ Blokli Client   │   │  │
│  │  │ (periodic    │  │ (GraphQL)       │   │  │
│  │  │  purge)      │  │                 │   │  │
│  │  └──────────────┘  └─────────────────┘   │  │
│  └──────────────────────────────────────────┘  │
│           │                                    │
│           ▼                                    │
│  ┌──────────────────────────────────────────┐  │
│  │        Embedded Database (SQLite)        │  │
│  │                                          │  │
│  │  debug_sessions      telemetry_spans     │  │
│  │  debug_session_peers telemetry_metrics   │  │
│  │                      telemetry_logs      │  │
│  └──────────────────────────────────────────┘  │
└────────────────────────────────────────────────┘
```

---

## 3. Core Concepts

### 3.1 Peer Tracking

HOSE maintains an **in-memory presence registry** of every peer (HOPRd node)
that has sent telemetry. For each peer, it records an identifier and a last-seen
timestamp.

This registry is the foundation for the operator experience: it shows which
nodes are alive and reporting, without storing any telemetry content. It is
deliberately transient — peer presence does not survive a restart, which is
acceptable for a debugging tool.

Peer identity is extracted from standard OTLP resource attributes that HOPRd
includes in its telemetry exports.

### 3.2 HOPR Session Tracking

Alongside peer tracking, HOSE maintains an **in-memory registry of active HOPR
sessions** extracted from OTLP telemetry. HOPRd nodes emit telemetry that
contains session-level attributes — session identifiers, protocol type, hop
count, and the roles of participating peers (entry, relay, exit).

By correlating these attributes across telemetry from multiple peers, HOSE
builds a live view of which HOPR sessions are active across the network and
which peers participate in each one. This is the key to the operator workflow:
rather than manually selecting individual peers, the operator can **select a
HOPR session** and HOSE automatically identifies all peers involved in it.

Like the peer presence registry, the session registry is transient — it does not
survive a restart. It reflects the current state of the network as observed
through telemetry.

### 3.3 Two-Mode Ingestion (Discard / Retain)

In its default state, HOSE **discards all telemetry payloads**. It only updates
the peer and session registries. This means normal operation has effectively
zero storage cost.

When an operator creates a debug session, HOSE switches the selected peers to
**retain mode**: incoming telemetry for those peers is buffered and written to
the database. When the debug session ends, the peers revert to discard mode
(unless retained by another concurrent session). This selective approach avoids
the storage, query, and cleanup burden of an always-on telemetry store while
still providing full capture capability on demand.

The routing decision is made by the **Peer Router**, which maintains a mapping
of peer IDs to the set of active debug sessions that have claimed them. A peer
with no entries in the router has its telemetry discarded; a peer with one or
more entries has its telemetry enqueued for all claiming sessions.

### 3.4 Debug Sessions

A debug session is an **operator-initiated, time-bounded** capture window. The
primary workflow is session-oriented:

1. The operator browses active HOPR sessions in the session registry
2. Selects a HOPR session to debug — HOSE automatically resolves all
   participating peers (entry, relays, exit)
3. Creates a named debug session
4. Inspects the captured telemetry (spans, metrics, logs) as it accumulates
5. Ends the debug session when done

Alternatively, the operator can manually select individual peers if they want to
capture telemetry outside the context of a specific HOPR session.

A peer can be part of multiple concurrent debug sessions. Session data remains
queryable after the session ends, until the periodic cleanup removes it after
the configured retention period.

### 3.5 Batched Write Buffer

Telemetry ingestion must not block the gRPC receiver. HOSE interposes a **write
buffer** between the ingestion path and the database:

- The gRPC handler enqueues records into a bounded in-memory channel
  (non-blocking)
- A background task drains the channel and flushes records to the database in
  batched transactions
- Batches are flushed either when a size threshold is reached or a time interval
  elapses, whichever comes first

If the channel is full (ingestion outpaces writing), records are dropped and a
warning is logged. This is the "best-effort ingestion" guarantee — acceptable
for a debugging tool, where some data loss under extreme load is preferable to
back-pressuring the telemetry pipeline.

### 3.6 Data Enrichment from the Blockchain Layer

HOPR is a token-incentivized network: sessions traverse **payment channels**
that have on-chain state (balances, ticket counters, closure status). A session
failure might not be a software bug — it might be an underfunded channel or a
channel pending closure. HOSE bridges this gap by querying a blockchain indexer
for on-chain channel data and correlating it with the peers visible in the
telemetry stream. The full details of this integration are described in §5.

---

## 4. Data Flow

### 4.1 Idle Operation (Discard Mode)

```
HOPRd → Collector → gRPC → HOSE receiver
                              │
                              ├─ Extract peer identity from OTLP resource attributes
                              ├─ Update peer presence registry (in-memory)
                              ├─ Extract HOPR session attributes (if present)
                              ├─ Update session registry (in-memory)
                              ├─ Check routing decision → "discard"
                              └─ Drop payload, return success to collector
```

The collector sees a successful export. HOSE has updated its peer and session
registries. No disk I/O occurred.

### 4.2 Active Debugging (Retain Mode)

```
1. Operator selects a HOPR session (or individual peers) via the web interface
   ├─ HOSE resolves all participating peers from the session registry
   ├─ Debug session and resolved peers recorded in database
   └─ Peer router updated: resolved peers now marked for retention

2. Telemetry arrives for a retained peer
   ├─ Update peer presence registry
   ├─ Check routing decision → "retain for session(s) X, Y"
   ├─ Enqueue record into write buffer (non-blocking)
   └─ Return success to collector

3. Write buffer background task
   ├─ Collects records from the channel
   ├─ Flushes to database in batched transactions
   └─ Repeats on size or time threshold

4. Operator queries captured data
   ├─ Aggregate statistics (counts by type)
   ├─ Paginated span/metric/log listings
   └─ Live event stream pushes peer-seen and session-updated notifications
```

### 4.3 Session Teardown and Cleanup

```
1. Operator ends the session via the web interface
   ├─ Session status updated in database
   └─ Peer router updated: peers revert to discard (unless retained by another session)

2. Data remains queryable until cleanup

3. Periodic cleanup task (runs hourly)
   └─ Removes completed sessions older than the retention threshold
      (cascade-deletes associated telemetry data)
```

---

## 5. External Integration: Blokli Blockchain Indexer

HOSE queries the Blokli blockchain indexer (via GraphQL) as its **primary source
of channel data**, providing a network-wide view derived directly from the smart
contract layer. This is the only external service HOSE queries — all other data
(peer presence, HOPR session state, telemetry) is derived passively from the
OTLP stream, requiring no API credentials for individual nodes.

### 5.1 What Blokli Provides

- **Channel state** — status (open, closed, pending closure), balances, closure
  timestamps
- **Economic primitives** — channel epochs, ticket indices, token balances
- **Identity resolution** — mapping between blockchain key IDs and peer IDs,
  enabling HOSE to correlate on-chain channel data with the peers visible in the
  OTLP telemetry stream
- **Real-time subscriptions** — live notifications when channel state changes
  on-chain, not limited to point-in-time queries

### 5.2 Why This Matters for Debugging

OTLP telemetry tells the operator _what happened_ at the application layer
(session establishment failed, packets were dropped, latency spiked). Channel
data from Blokli tells the operator _why it might have happened_ at the economic
layer:

- Is the payment channel between relay and exit underfunded?
- Is a channel pending closure, preventing new ticket redemptions?
- Has the ticket index advanced as expected, or is there a ticket validation
  issue?

By presenting both data sources side-by-side, HOSE enables operators to diagnose
issues that span the boundary between protocol behavior and on-chain economics —
a class of problems that neither telemetry nor blockchain data can explain
alone.

### 5.3 Identity Bridging

The blockchain layer uses numeric key IDs to identify nodes, while OTLP
telemetry uses peer IDs (derived from the node's packet key). HOSE resolves this
mismatch by querying the indexer for account data and extracting peer IDs from
registered multiaddresses or packet keys. This mapping is cached in-memory and
refreshed on demand, so that channel source/destination pairs are displayed
using the same peer identifiers visible in the telemetry stream.

---

## 6. Web Interface

### 6.1 HTML Pages

| Route          | Purpose                                                              |
| -------------- | -------------------------------------------------------------------- |
| Dashboard      | Active peer count, active HOPR sessions, system health               |
| Peers          | All peers that have reported telemetry, with last-seen times         |
| HOPR sessions  | Active HOPR sessions observed in telemetry, with participating peers |
| Debug sessions | All operator-created debug sessions with their status                |
| Create debug   | Select a HOPR session (or individual peers) to start debugging       |
| Debug detail   | Per-peer telemetry statistics, drill-down into spans/metrics/logs    |

### 6.2 JSON API

| Purpose              | Description                                                                    |
| -------------------- | ------------------------------------------------------------------------------ |
| Readiness probe      | `GET /readyz` - 200 when DB and gRPC are healthy, 503 otherwise                |
| Liveness probe       | `GET /livez` - 200 if the HTTP server can respond                              |
| Peer listing         | All tracked peers                                                              |
| HOPR session listing | Active HOPR sessions with participating peers and metadata                     |
| Debug session CRUD   | Create, read, update status of debug sessions                                  |
| Telemetry queries    | Paginated spans, metrics, and logs for a given debug session                   |
| Channel data         | Enriched HOPR channel information with on-chain state (via blockchain indexer) |

### 6.3 Live Event Stream

A unidirectional server-to-browser event stream (SSE) delivers:

- **Peer seen** — notifies when a peer reports telemetry
- **HOPR session observed** — notifies when a new HOPR session appears or its
  participants change
- **Debug session updated** — notifies when debug session state or statistics
  change
- **Telemetry rate** — periodic throughput indicator

Clients that fall behind drop events rather than buffering unboundedly.

---

## 7. Design Decisions & Trade-offs

| Decision                  | Instead of          | Rationale                                                                                                       |
| ------------------------- | ------------------- | --------------------------------------------------------------------------------------------------------------- |
| **Passive reception**     | Active polling      | Eliminates timing issues, leverages existing OTLP pipeline, scales to many nodes without per-node configuration |
| **Web UI**                | Terminal UI         | Multi-user access, richer visualization, persistent sessions, no terminal dependency                            |
| **gRPC for OTLP**         | HTTP/protobuf       | Matches existing OTLP infrastructure, avoids requiring collector reconfiguration                                |
| **Embedded database**     | No persistence      | Debug data survives page refreshes, enables post-hoc analysis within retention window                           |
| **Selective retention**   | Always-on storage   | Zero storage cost in normal operation, avoids the burden of storing/querying/cleaning all telemetry             |
| **Single binary**         | Microservices       | Simpler deployment for a debugging tool — no external database server, no separate frontend build               |
| **Best-effort ingestion** | Guaranteed delivery | Debugging data can tolerate some loss; the ingestion path must not back-pressure the telemetry pipeline         |
| **Server-Sent Events**    | WebSocket           | Update flow is unidirectional (server → browser); SSE is simpler, reconnects automatically per browser spec     |
| **Server-rendered HTML**  | Single-page app     | Avoids a separate frontend build pipeline; dynamic updates use the JSON API + SSE from vanilla JavaScript       |

---

## 8. Operational Characteristics

### 8.1 Deployment

- Single binary, single process
- Creates a database file on disk at a configured path
- Listens on two ports: one for gRPC (OTLP), one for HTTP (web UI + API)
- No external services required (database is embedded)

### 8.2 Health Probes

HOSE exposes two HTTP endpoints for orchestration systems (Kubernetes, Docker,
etc.):

- **Readiness** (`GET /readyz`) — returns 200 with a JSON body when the SQLite
  database is reachable and the gRPC OTLP listener has bound. Returns 503 with
  per-check status when any subsystem is unavailable. Orchestrators should use
  this to decide whether to route traffic to the instance.
- **Liveness** (`GET /livez`) — returns 200 if the HTTP server can respond at
  all. A failure here indicates the process is deadlocked or unresponsive and
  should be restarted.

### 8.3 Configuration Surface

- **Indexer endpoint** — for blockchain channel data from Blokli
- **Listen addresses** — for the gRPC and HTTP servers
- **Database location** — path to the embedded database file
- **Retention period** — how long completed session data is kept before cleanup
- **Write buffer tuning** — batch size and flush interval for the ingestion path

### 8.4 Data Retention

- Peer and HOPR session registries: in-memory only, lost on restart
- Debug session data: persisted in the embedded database, cleaned up
  periodically after the session ends and the retention window expires
- No long-term archival — HOSE is a debugging tool, not an observability
  platform

### 8.5 Error Handling Philosophy

- **Ingestion path**: best-effort. The gRPC receiver always returns success to
  the collector, even if individual records are dropped due to buffer pressure
  or write failures. The telemetry pipeline must not be disrupted by HOSE
  issues.
- **Write path**: failures are logged but do not crash the service. The write
  buffer has a finite capacity; overflow is handled by dropping records.
- **Web interface**: structured error responses with appropriate HTTP status
  codes.
- **External API calls**: failures in indexer queries are surfaced to the
  operator but do not affect the core ingestion/storage path.

### 8.6 Concurrency Model

- The gRPC and HTTP servers run concurrently in the same process, sharing state
  via reference-counted pointers
- The gRPC handler is non-blocking: it enqueues work to the write buffer and
  returns immediately
- A single background task handles batched writes to the database
- A single background task handles periodic cleanup of expired sessions
- Each browser connection subscribes to a broadcast channel for live events;
  slow clients drop events
