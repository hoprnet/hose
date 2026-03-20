# pre-commit-check.nix - Pre-commit hooks configuration package
#
# Defines the pre-commit hooks that run automatically before each commit
# to ensure code quality, formatting, and basic validation.

{
  pre-commit,
  system,
  config,
  pkgs,
}:

pre-commit.lib.${system}.run {
  src = ./../..; # Root of the project

  # Configure the pre-commit hooks to run
  hooks = {
    # Code formatting via treefmt
    treefmt.enable = true;
    treefmt.package = config.treefmt.build.wrapper;

    # Clippy runs in CI via `nix develop -c cargo clippy` using the correct
    # rust-overlay toolchain.  The pre-commit clippy uses nixpkgs' rustc which
    # is a different version and cannot read artifacts built by the dev shell.
    clippy.enable = false;

    # Shell script validation
    check-executables-have-shebangs.enable = true;
    check-shebang-scripts-are-executable.enable = true;

    # File system checks
    check-case-conflicts.enable = true;
    check-symlinks.enable = true;
    check-merge-conflicts.enable = true;
    check-added-large-files.enable = true;

    # Commit message formatting
    commitizen.enable = true;
  };

  # Tools available to the pre-commit environment
  tools = pkgs;
}
