# checks.nix - CI/CD quality checks
#
# Defines automated checks that run in CI to ensure code quality.
# Hose runs clippy via CI workflow commands rather than as Nix check derivations.

{ pkgs }:

{
  # Checks are handled directly in CI workflows (cargo clippy, cargo test)
}
