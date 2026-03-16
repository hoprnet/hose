{ pkgs, ... }:

{
  # Rust toolchain
  languages.rust = {
    enable = true;
    channel = "stable";
  };

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
  };

  # Shell initialization
  enterShell = ''
    echo "HOPR Session Debugger dev environment loaded"
    echo "Rust $(rustc --version)"
    echo "Beads (bd) is available for task tracking"
  '';
}
