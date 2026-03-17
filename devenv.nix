{ pkgs, ... }:

{
  # Rust toolchain
  languages.rust.enable = true;

  # Project-specific packages
  packages = with pkgs; [
    beads
    protobuf
    pkg-config
    openssl
    sqlite
  ];

  # Environment variables
  env = {
    PROJECT_ROOT = builtins.toString ./.;
    PROTOC = "${pkgs.protobuf}/bin/protoc";
  };

  # Pre-commit hooks
  pre-commit.hooks = {
    rustfmt.enable = true;
    clippy.enable = true;
  };

  # Scripts available in the devshell
  scripts = {
    hose-dev.exec = ''
      echo "Starting HOSE dev server (HTTP :8080, gRPC :4317)..."
      export RUST_LOG=''${RUST_LOG:-info,hose=debug}
      cargo run
    '';
    hose-dev.description = "Build and run HOSE with sensible dev defaults";

    hose-gen.exec = ''
      echo "Starting OTLP trace generator → localhost:4317..."
      cargo run --example trace_generator -- "$@"
    '';
    hose-gen.description = "Send synthetic OTLP traces to the local HOSE instance";
  };

  # Shell initialization
  enterShell = ''
    echo "HOPR Session Debugger dev environment loaded"
    echo "Rust $(rustc --version)"
    echo "Beads (bd) is available for task tracking"
  '';
}
