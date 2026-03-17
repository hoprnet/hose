# ============================================================
# Stage 1: ReScript build
# Compiles ReScript sources to JavaScript for the web UI
# ============================================================
FROM node:22-slim AS rescript-build

WORKDIR /app

# Install dependencies first for better layer caching
COPY package.json package-lock.json rescript.json ./
RUN npm ci

# Copy ReScript source and build
COPY rescript/ rescript/
RUN npm run res:build

# Assemble the JS output into static/ layout
RUN mkdir -p static/js static/js/rescript \
    && cp lib/es6/rescript/src/*.mjs static/js/ \
    && cp node_modules/rescript/lib/es6/js_dict.js \
       node_modules/rescript/lib/es6/js_json.js \
       node_modules/rescript/lib/es6/js_promise.js \
       node_modules/rescript/lib/es6/caml_option.js \
       node_modules/rescript/lib/es6/curry.js \
       node_modules/rescript/lib/es6/caml_array.js \
       static/js/rescript/

# ============================================================
# Stage 2: Rust build
# Compiles the Rust binary with all static assets embedded
# ============================================================
FROM rust:1.85-slim-bookworm AS rust-build

# Install build dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    protobuf-compiler \
    pkg-config \
    libssl-dev \
    libsqlite3-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy manifests first for dependency caching
COPY Cargo.toml Cargo.lock ./

# Create a dummy main.rs to pre-build dependencies
RUN mkdir src \
    && echo "fn main() {}" > src/main.rs \
    && cargo build --release 2>/dev/null || true \
    && rm -rf src

# Copy the actual source code
COPY src/ src/
COPY migrations/ migrations/
COPY templates/ templates/

# Copy static assets from the ReScript build stage,
# then overlay with any existing static files from the repo
COPY --from=rescript-build /app/static/ static/
COPY static/ static/

# Build the release binary
RUN cargo build --release

# ============================================================
# Stage 3: Runtime
# Minimal image with only runtime dependencies
# ============================================================
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
    libssl3 \
    libsqlite3-0 \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Create a non-root user for running the application
RUN useradd --create-home --shell /bin/bash hose

WORKDIR /app

# Copy the compiled binary
COPY --from=rust-build /app/target/release/hose ./

# Copy static assets (CSS, JS, etc.)
COPY --from=rust-build /app/static/ static/

# Copy HTML templates
COPY --from=rust-build /app/templates/ templates/

# Copy database migrations
COPY --from=rust-build /app/migrations/ migrations/

# Create the data directory for SQLite persistence
RUN mkdir -p /data && chown hose:hose /data

# Set sensible default environment variables
ENV HOSE_GRPC_LISTEN=0.0.0.0:4317 \
    HOSE_HTTP_LISTEN=0.0.0.0:8080 \
    HOSE_DATABASE_PATH=/data/hose.db \
    HOSE_RETENTION_HOURS=24 \
    HOSE_WRITE_BUFFER_SIZE=1000 \
    HOSE_WRITE_BUFFER_FLUSH_SECS=5 \
    RUST_LOG=info

# HTTP (web UI + REST API) and gRPC (OTLP receiver)
EXPOSE 8080 4317

USER hose

CMD ["./hose"]
