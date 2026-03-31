# HOSE development recipes

# Show available recipes
default:
    @just --list

# Compile ReScript modules and copy to static/js/
res-build:
    #!/usr/bin/env bash
    set -euo pipefail
    echo "Building ReScript modules..."
    npm run res:build
    mkdir -p static/js static/js/rescript
    cp lib/es6/rescript/src/*.mjs static/js/
    cp node_modules/rescript/lib/es6/js_dict.js \
       node_modules/rescript/lib/es6/js_json.js \
       node_modules/rescript/lib/es6/js_promise.js \
       node_modules/rescript/lib/es6/caml_option.js \
       node_modules/rescript/lib/es6/curry.js \
       node_modules/rescript/lib/es6/caml_array.js \
       static/js/rescript/
    echo "ReScript build complete -> static/js/"

# Watch ReScript files and rebuild on change
res-watch:
    #!/usr/bin/env bash
    set -euo pipefail
    echo "Watching ReScript modules (Ctrl+C to stop)..."
    npm run res:watch &
    WATCH_PID=$!
    trap "kill $WATCH_PID 2>/dev/null" EXIT
    while true; do
        sleep 2
        if [ -d lib/es6/rescript/src ]; then
            mkdir -p static/js
            cp lib/es6/rescript/src/*.mjs static/js/ 2>/dev/null || true
        fi
    done

# Build ReScript then start HOSE dev server (HTTP :8080, gRPC :4317)
dev:
    #!/usr/bin/env bash
    set -euo pipefail
    echo "Building ReScript modules..."
    if [ -f package.json ] && [ -d rescript/src ]; then
        [ -d node_modules ] || npm install --silent
        npm run res:build 2>&1
        mkdir -p static/js static/js/rescript
        cp lib/es6/rescript/src/*.mjs static/js/ 2>/dev/null || true
        cp node_modules/rescript/lib/es6/js_dict.js \
           node_modules/rescript/lib/es6/js_json.js \
           node_modules/rescript/lib/es6/js_promise.js \
           node_modules/rescript/lib/es6/caml_option.js \
           node_modules/rescript/lib/es6/curry.js \
           node_modules/rescript/lib/es6/caml_array.js \
           static/js/rescript/ 2>/dev/null || true
    fi
    echo "Starting HOSE dev server (HTTP :8080, gRPC :4317)..."
    export RUST_LOG="${RUST_LOG:-info,hose=debug}"
    cargo run

# Send synthetic OTLP traces to the local HOSE instance
gen *ARGS:
    #!/usr/bin/env bash
    set -euo pipefail
    echo "Starting OTLP trace generator -> localhost:4317..."
    cargo run --example trace_generator -- {{ARGS}}

# Lint and render Helm chart templates
helm-test *ARGS:
    #!/usr/bin/env bash
    set -euo pipefail
    echo "Linting chart..."
    helm lint charts/hose/ {{ARGS}}
    echo ""
    echo "Rendering templates..."
    helm template test charts/hose/ {{ARGS}}

# Update GitHub Actions SHA pins to latest releases
gha-update *ARGS:
    #!/usr/bin/env bash
    set -euo pipefail
    "$PROJECT_ROOT/scripts/gha-update.ts" {{ARGS}}
