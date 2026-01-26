{ pkgs, ... }:

{
  # Project-specific packages
  packages = with pkgs; [
    beads
  ];

  # Environment variables
  env = {
    PROJECT_ROOT = builtins.toString ./.;
  };

  # Pre-commit hooks (optional)
  # pre-commit.hooks = {
  #   nixpkgs-fmt.enable = true;
  # };

  # Scripts available in the devshell
  scripts = {
  };

  # Shell initialization
  enterShell = ''
    echo "🔨 HOPR Session Debugger dev environment loaded"
    echo "📊 Beads (bd) is available for task tracking"
    echo ""
    echo "Run 'bd quickstart' to see how to use beads"
  '';
}
