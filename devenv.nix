{
  pkgs,
  lib,
  config,
  inputs,
  ...
}:

let
  isBuilding = config.container.isBuilding;

  craneLib = inputs.crane.mkLib pkgs;

  rescriptAssets = import ./nix/rescript.nix { inherit pkgs; };

  hoseBuild =
    profile:
    import ./nix/hose.nix {
      inherit pkgs craneLib rescriptAssets;
      inherit profile;
    };

  hoseRelease = hoseBuild "release";
  hoseDev = hoseBuild "dev";

  mkAppRoot =
    hosePkg:
    pkgs.runCommand "hose-root" { } ''
      mkdir -p $out/app $out/data $out/tmp
      cp ${hosePkg}/bin/hose $out/app/
      cp -r ${hosePkg}/share/hose/static $out/app/
      cp -r ${hosePkg}/share/hose/migrations $out/app/
      cp -r ${hosePkg}/share/hose/templates $out/app/
    '';
in
{
  # Rust toolchain (only in dev shell, not when building containers)
  languages.rust.enable = !isBuilding;

  # Node.js (only in dev shell)
  languages.javascript.enable = !isBuilding;
  languages.javascript.package = pkgs.nodejs_22;

  # Project-specific packages
  packages =
    if isBuilding then
      [ pkgs.cacert ]
    else
      with pkgs;
      [
        beads
        protobuf
        pkg-config
        openssl
        sqlite
        treefmt
        deno
      ];

  # Environment variables
  env =
    if isBuilding then
      {
        HOSE_GRPC_LISTEN = "0.0.0.0:4317";
        HOSE_HTTP_LISTEN = "0.0.0.0:8080";
        HOSE_DATABASE_PATH = "/data/hose.db";
        HOSE_RETENTION_HOURS = "24";
        HOSE_WRITE_BUFFER_SIZE = "1000";
        HOSE_WRITE_BUFFER_FLUSH_SECS = "5";
        RUST_LOG = "info";
      }
    else
      {
        PROJECT_ROOT = builtins.toString ./.;
        PROTOC = "${pkgs.protobuf}/bin/protoc";
      };

  # Git hooks
  git-hooks.hooks = {
    treefmt.enable = true;
    clippy.enable = true;
  };

  # Scripts available in the devshell
  scripts = {
    hose-res-build.exec = ''
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
      echo "ReScript build complete → static/js/"
    '';
    hose-res-build.description = "Compile ReScript modules and copy to static/js/";

    hose-res-watch.exec = ''
      echo "Watching ReScript modules (Ctrl+C to stop)..."
      npm run res:watch &
      WATCH_PID=$!
      trap "kill $WATCH_PID 2>/dev/null" EXIT
      # Watch for changes and copy
      while true; do
        sleep 2
        if [ -d lib/es6/rescript/src ]; then
          mkdir -p static/js
          cp lib/es6/rescript/src/*.mjs static/js/ 2>/dev/null
        fi
      done
    '';
    hose-res-watch.description = "Watch ReScript files and rebuild on change";

    hose-dev.exec = ''
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
      export RUST_LOG=''${RUST_LOG:-info,hose=debug}
      cargo run
    '';
    hose-dev.description = "Build ReScript, then build and run HOSE with sensible dev defaults";

    hose-gen.exec = ''
      echo "Starting OTLP trace generator → localhost:4317..."
      cargo run --example trace_generator -- "$@"
    '';
    hose-gen.description = "Send synthetic OTLP traces to the local HOSE instance";

    gha-update.exec = ''"$PROJECT_ROOT/scripts/gha-update.ts" "$@"'';
    gha-update.description = "Update GitHub Actions SHA pins to latest releases (dry-run by default, --apply to write)";
  };

  # Shell initialization
  enterShell = ''
    echo "HOPR Session Debugger dev environment loaded"
    echo "Rust $(rustc --version)"
    echo "Node $(node --version)"
    echo "Beads (bd) is available for task tracking"
  '';

  # Container definitions
  containers."hose" = {
    name = "hopr/hose";
    copyToRoot = mkAppRoot hoseRelease;
    startupCommand = "cd /app && exec ./hose";
  };

  containers."hose-dev" = {
    name = "hopr/hose-dev";
    copyToRoot = mkAppRoot hoseDev;
    startupCommand = "cd /app && exec ./hose";
  };

  cachix.pull = [ "hose" ];
}
