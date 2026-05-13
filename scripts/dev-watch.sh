#!/usr/bin/env bash
set -euo pipefail

CONFIG_PATH="${1:-}"
POLL_INTERVAL_SECS="${POLL_INTERVAL_SECS:-1}"
RESTART_DELAY_SECS="${RESTART_DELAY_SECS:-1}"

SERVER_PID=""

build_rescript() {
  if [ -f package.json ] && [ -d rescript/src ]; then
    [ -d node_modules ] || bun install --silent
    bun run res:build
    mkdir -p static/js static/js/rescript
    cp lib/es6/rescript/src/*.mjs static/js/ 2>/dev/null || true
    cp node_modules/rescript/lib/es6/*.js static/js/rescript/ 2>/dev/null || true
  fi
}

server_start() {
  echo "Starting HOSE dev server (HTTP :8080, gRPC :4317)..."
  export RUST_LOG="${RUST_LOG:-info,hose=debug}"

  if [ -n "$CONFIG_PATH" ]; then
    cargo run -- --config "$CONFIG_PATH" &
  else
    cargo run &
  fi

  SERVER_PID=$!
  echo "Server PID: $SERVER_PID"
}

server_stop() {
  if [ -n "${SERVER_PID:-}" ] && kill -0 "$SERVER_PID" 2>/dev/null; then
    echo "Stopping server PID $SERVER_PID..."
    kill "$SERVER_PID" 2>/dev/null || true
    wait "$SERVER_PID" 2>/dev/null || true
  fi
  SERVER_PID=""
}

cleanup() {
  server_stop
}

collect_signature() {
  {
    find src -type f \( -name '*.rs' \) -print0 2>/dev/null || true
    find templates -type f \( -name '*.html' \) -print0 2>/dev/null || true
    find static -type f \( -name '*.js' -o -name '*.css' \) -print0 2>/dev/null || true
    find rescript/src -type f \( -name '*.res' -o -name '*.resi' \) -print0 2>/dev/null || true
    find migrations -type f -print0 2>/dev/null || true
    [ -f Cargo.toml ] && printf '%s\0' Cargo.toml
    [ -f Cargo.lock ] && printf '%s\0' Cargo.lock
    [ -f rescript.json ] && printf '%s\0' rescript.json
    [ -f package.json ] && printf '%s\0' package.json
  } | xargs -0r stat -c '%n|%s|%Y' 2>/dev/null | LC_ALL=C sort | sha256sum | awk '{print $1}'
}

changed_rescript() {
  local current_sig
  current_sig="$(
    {
      find rescript/src -type f \( -name '*.res' -o -name '*.resi' \) -print0 2>/dev/null || true
      [ -f rescript.json ] && printf '%s\0' rescript.json
      [ -f package.json ] && printf '%s\0' package.json
    } | xargs -0r stat -c '%n|%s|%Y' 2>/dev/null | LC_ALL=C sort | sha256sum | awk '{print $1}'
  )"

  if [ "$current_sig" != "${RES_SIG:-}" ]; then
    RES_SIG="$current_sig"
    return 0
  fi
  return 1
}

trap cleanup EXIT INT TERM

echo "Building ReScript modules..."
build_rescript

initial_sig="$(collect_signature)"
RES_SIG="$(
  {
    find rescript/src -type f \( -name '*.res' -o -name '*.resi' \) -print0 2>/dev/null || true
    [ -f rescript.json ] && printf '%s\0' rescript.json
    [ -f package.json ] && printf '%s\0' package.json
  } | xargs -0r stat -c '%n|%s|%Y' 2>/dev/null | LC_ALL=C sort | sha256sum | awk '{print $1}'
)"

server_start

echo "Hot reload enabled. Watching Rust, ReScript, JS, CSS, and HTML files..."
while true; do
  sleep "$POLL_INTERVAL_SECS"
  next_sig="$(collect_signature)"

  if [ "$next_sig" != "$initial_sig" ]; then
    echo "Detected source changes. Restarting..."

    if changed_rescript; then
      echo "ReScript sources changed; rebuilding frontend assets..."
      build_rescript
    fi

    server_stop
    server_start
    initial_sig="$next_sig"
  fi

  if [ -n "${SERVER_PID:-}" ] && ! kill -0 "$SERVER_PID" 2>/dev/null; then
    echo "Server exited. Restarting in ${RESTART_DELAY_SECS}s..."
    sleep "$RESTART_DELAY_SECS"
    server_start
  fi
done
