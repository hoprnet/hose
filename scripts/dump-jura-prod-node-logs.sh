#!/usr/bin/env bash
#
# Dump hoprd logs from every jura-prod node -- exits AND relayers -- for a fixed
# UTC window (default: yesterday 09:00-21:00 UTC) into per-node files on this
# machine.
#
# WHY RELAYERS MATTER:
#   A relayer whose outgoing channel has drained cannot mint a ticket, so it
#   drops the packet silently -- nothing surfaces in the exit's log. Symptoms
#   that look like exit-side SURB starvation are often a relay dropping the
#   return leg. Always pull both roles before blaming an exit.
#
# WHY NOT `docker logs` OVER SSH:
#   The hoprd container uses the `fluentd` docker log driver and ships logs to
#   Google Cloud Logging (stackdriver) via fluentbit. docker's local log cache
#   only retains the last few hours (high log volume + rotation), so yesterday's
#   window is already gone from `docker logs`. The durable copy lives in Cloud
#   Logging, so we read from there instead. No ssh / scp required.
#
#   logName : projects/<project>/logs/output.hoprd
#   node    : jsonPayload.node_name = <instance name>
#
# Output: ./jura-prod-logs/<day>/<node>.json  (newline-delimited JSON entries,
#         chronological order). run.log holds status + per-node counts.
#
# WARNING: volume is large (~50-60k entries/hour/node -> a 12h window can be
# hundreds of MB and several minutes per node). Narrow the window or add a
# SEVERITY filter if you only need warnings/errors.

set -uo pipefail   # NOT -e: one bad node must not abort the whole run

# jura-prod nodes live in gnosisvpn-production. gnosisvpn-staging holds the
# jura-staging fleet under identical instance names, so a wrong PROJECT here
# yields plausible-looking logs from the wrong network.
PROJECT="${PROJECT:-gnosisvpn-production}"
EXPECTED_NETWORK="${EXPECTED_NETWORK:-jura-prod}"
LOG_NAME="projects/${PROJECT}/logs/output.hoprd"

# Max entries pulled per node. gcloud logging read defaults to 1000; bump high
# enough to cover the window. Set to 0 for unlimited (slow).
MAX_ENTRIES="${MAX_ENTRIES:-1000000}"

# Optional extra filter, e.g. SEVERITY='AND severity>=WARNING'
SEVERITY="${SEVERITY:-}"

# --- time window (UTC) --------------------------------------------------------
if date -u -v-1d +%Y-%m-%d >/dev/null 2>&1; then
  YESTERDAY="$(date -u -v-1d +%Y-%m-%d)"
else
  YESTERDAY="$(date -u -d 'yesterday' +%Y-%m-%d)"
fi

# Args: $1 = day (YYYY-MM-DD, default yesterday)
# Env:  ROLE      = "exit" or "relayer" to pull only that role (default: both)
#       ONLY_NODE = substring to run a single matching node (for testing)
#       SINCE     = RFC3339 window start (default <day>T09:00:00Z)
#       UNTIL     = RFC3339 window end   (default <day>T21:00:00Z)
DAY="${1:-$YESTERDAY}"
SINCE="${SINCE:-${DAY}T09:00:00Z}"
UNTIL="${UNTIL:-${DAY}T21:00:00Z}"
ONLY_NODE="${ONLY_NODE:-}"
ROLE="${ROLE:-}"

# --- output dir + run log -----------------------------------------------------
OUT_DIR="./jura-prod-logs/${DAY}"
mkdir -p "$OUT_DIR"
RUN_LOG="${OUT_DIR}/run.log"
: >"$RUN_LOG"

log() { echo "[$(date -u +%H:%M:%S)] $*" | tee -a "$RUN_LOG"; }

# jura-prod nodes. ROLE (below) filters this list; ONLY_NODE narrows it further.
NODES=(
  gnosisvpn-exit-node-london-01
  gnosisvpn-exit-node-amsterdam-01
  gnosisvpn-exit-node-iowa-01
  gnosisvpn-exit-node-sao-paulo-01
  gnosisvpn-exit-node-seoul-01
  gnosisvpn-exit-node-mumbai-01
  gnosisvpn-exit-node-sydney-01
  gnosisvpn-relayer-node-columbus-01
  gnosisvpn-relayer-node-delhi-01
  gnosisvpn-relayer-node-madrid-01
  gnosisvpn-relayer-node-melbourne-01
  gnosisvpn-relayer-node-sao-paulo-01
  gnosisvpn-relayer-node-stockholm-01
  gnosisvpn-relayer-node-tokyo-01
)

log "Source: Cloud Logging (${LOG_NAME})"
[ -n "$ROLE" ] && log "ROLE filter: ${ROLE}"
log "Window: ${SINCE} -> ${UNTIL} (UTC)"
log "Output: ${OUT_DIR}"
[ -n "$ONLY_NODE" ] && log "ONLY_NODE filter: ${ONLY_NODE}"
[ -n "$SEVERITY" ] && log "Severity filter: ${SEVERITY}"
echo

declare -a SUMMARY=()

for NAME in "${NODES[@]}"; do
  if [ -n "$ROLE" ] && [[ "$NAME" != *"-${ROLE}-node-"* ]]; then
    continue
  fi
  if [ -n "$ONLY_NODE" ] && [[ "$NAME" != *"$ONLY_NODE"* ]]; then
    continue
  fi

  LOCAL_LOG="${OUT_DIR}/${NAME}.json"
  FILTER="logName=\"${LOG_NAME}\" AND jsonPayload.node_name=\"${NAME}\" AND timestamp>=\"${SINCE}\" AND timestamp<=\"${UNTIL}\" ${SEVERITY}"

  log "==> ${NAME}"

  # --order=asc = chronological. --format=json emits a JSON array; we keep it as
  # the full structured entries so nothing is lost.
  if ! gcloud logging read "$FILTER" \
        --project "$PROJECT" \
        --order=asc \
        --limit="$MAX_ENTRIES" \
        --format=json \
        >"$LOCAL_LOG" 2>>"$RUN_LOG"; then
    log "    FAIL: gcloud logging read failed for ${NAME} (see ${RUN_LOG})"
    SUMMARY+=("FAIL  ${NAME}")
    continue
  fi

  # Count entries and warn on empty/at-cap results.
  count=$(grep -c '"insertId"' "$LOCAL_LOG" 2>/dev/null) || count=0
  bytes=$(wc -c <"$LOCAL_LOG" | tr -d ' ')

  net=$(grep -o '"node_network": "[^"]*"' "$LOCAL_LOG" | head -1 | cut -d'"' -f4)
  if [ -n "$net" ] && [ "$net" != "$EXPECTED_NETWORK" ]; then
    log "    WARN: ${NAME} reports node_network=${net}, expected ${EXPECTED_NETWORK} — wrong PROJECT?"
  fi

  if [ "$count" -eq 0 ]; then
    log "    WARN: ${NAME} returned 0 entries (${bytes}B)"
    SUMMARY+=("WARN  ${NAME} (0 entries)")
  elif [ "$count" -ge "$MAX_ENTRIES" ]; then
    log "    WARN: ${NAME} hit MAX_ENTRIES=${MAX_ENTRIES} — window likely TRUNCATED"
    SUMMARY+=("WARN  ${NAME} (${count} entries, TRUNCATED)")
  else
    log "    OK: ${LOCAL_LOG} (${count} entries, ${bytes}B)"
    SUMMARY+=("OK    ${NAME} (${count} entries)")
  fi
  echo
done

echo
log "===== SUMMARY ====="
for line in "${SUMMARY[@]}"; do
  log "  $line"
done
log "Logs + run.log in ${OUT_DIR}"
