# AI-aided GnosisVPN debugging

Tooling and workflow for debugging GnosisVPN by correlating a client's log
against `hoprd` logs pulled from the nodes on its path.

A client log tells you *that* something went wrong, never *where*. Localising a
fault to a hop means pulling logs from the exit and the relayers and joining
them to the client on the session pseudonym. That join is mechanical but
tedious — hundreds of megabytes of JSON across 14 nodes — which makes it a good
fit for an agent.

## Contents

| File | Purpose |
| ---- | ------- |
| `dump-jura-prod-node-logs.sh` | Pull `hoprd` logs from jura-prod nodes (7 exits + 7 relayers) out of Google Cloud Logging |

## Why not `docker logs`

The `hoprd` container uses the `fluentd` log driver and ships to Google Cloud
Logging via fluentbit. Docker's local cache only holds a few hours at jura-prod
log volume, so yesterday's window is already gone locally. The durable copy is
in Cloud Logging, which is what the script reads. No SSH required.

## Prerequisites

```bash
gcloud auth login          # SSO expires often; re-run when reads fail
```

The script targets project `gnosisvpn-staging` regardless of your gcloud
default. A read failing with `Reauthentication failed. cannot prompt during
non-interactive execution` means the token lapsed — log in again. An agent
cannot fix this itself and must ask you to run it.

## The script

```bash
./dump-jura-prod-node-logs.sh [YYYY-MM-DD]     # default: yesterday
```

| Env var | Effect |
| ------- | ------ |
| `ROLE` | `exit` or `relayer` — restrict to one role (default: both, 14 nodes) |
| `ONLY_NODE` | Substring match for a single node, e.g. `columbus` |
| `SINCE` / `UNTIL` | RFC3339 window bounds (default `<day>T09:00:00Z` → `T21:00:00Z`) |
| `SEVERITY` | Extra filter clause, e.g. `AND severity>=WARNING` |
| `MAX_ENTRIES` | Per-node cap (default 1000000; `0` = unlimited) |

Output lands in `./jura-prod-logs/<day>/<node>.json` as a JSON array of full
structured entries, chronological, with `run.log` holding per-node counts.

### Volume warning

The main practical constraint. One relayer at `severity>=ERROR` over a 12h
window returned **151 055 entries / 195 MB and took 5.5 minutes**. All 14 nodes
unfiltered will be tens of gigabytes and can fill a disk mid-run. Narrow with
`SINCE`/`UNTIL` and `SEVERITY` before widening.

## Workflow

### 1. Hand the agent the client log

The client log is the only artifact the user supplies. Everything else is
derived from it.

> Client log at `~/Downloads/gnosis_vpn-20260827-163450.log`. Validate it: build
> a timeline of connects, disconnects and failure events, and tell me which are
> genuine network faults versus self-inflicted (config errors, underfunding,
> restarts). Don't speculate about causes yet.

Ask for that split explicitly. Real client logs are noisy with operator
mistakes, and those will otherwise anchor the whole analysis.

What should come back:

- Failure events with timestamps, destination and trigger
- Which exits were in use over which intervals
- The **session pseudonym** for each main VPN session — the join key for
  everything that follows

Pseudonyms come from lines like:

```
INFO hopr_transport_session::manager: session is ready session_id=262e35d297902d172a24 surb_level=642
```

Health-check probe sessions produce thousands of these. The main VPN session is
the one immediately preceding `connection established successfully`.

### 2. Pull node logs around the failures

> For each failure window, pull logs with
> `scripts/dump-jura-prod-node-logs.sh`. Use `SINCE`/`UNTIL` to bracket the
> event tightly and start at `severity>=WARNING`; only drop the severity filter
> for the narrowest bursts. Pull relayers as well as exits.

Bracket the exit for a failure window:

```bash
SINCE="2026-08-27T13:30:00Z" UNTIL="2026-08-27T15:35:00Z" \
ROLE=exit ONLY_NODE=amsterdam SEVERITY='AND severity>=WARNING' \
  ./dump-jura-prod-node-logs.sh 2026-08-27
```

Then all seven relayers over the same window:

```bash
SINCE="2026-08-27T13:30:00Z" UNTIL="2026-08-27T15:35:00Z" \
ROLE=relayer SEVERITY='AND severity>=WARNING' \
  ./dump-jura-prod-node-logs.sh 2026-08-27
```

Large pulls take minutes and the JSON array is invalid until the read
completes — `jq` will fail with `Unfinished JSON term at EOF` on a file still
being written. Run them in the background and check completion before parsing.

**Pull relayers, not just exits.** This is the step most likely to be skipped,
and skipping it is what makes an investigation go wrong. A relayer whose
outgoing channel has drained cannot mint a ticket, so it drops the packet
**silently** — nothing surfaces in the exit's log. An exit-only investigation
sees clean exit logs, concludes the exit is fine, and stalls.

### 3. Correlate

> Join the exit and relayer logs to the client's failure bursts on the session
> pseudonym. For every client-side burst, tell me whether a node logged a
> matching drop. Where none did, say so explicitly — that gap is the finding.

Instructing the agent to **report unexplained bursts rather than quietly
matching what it can** is what makes this step work. A burst with no node-side
counterpart localises the loss to a hop that hasn't been pulled yet.

Useful cross-checks to request:

- Does the main session pseudonym appear in the exit's SURB evictions?
  Starvation (session consumes everything and runs dry) looks very different
  from waste (SURBs sitting unused until they expire).
- Are a node's drops sustained across the whole window, or only during the
  client's failure? Sustained means a background fault that predates the
  session.
- Do drop reasons agree across nodes? One `field_error` string repeating across
  many nodes points at shared infrastructure, not a node-local problem.
