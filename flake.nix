# flake.nix - HOPR Session Explorer Nix flake configuration
#
# This is the main entry point for the Nix flake. It uses the HOPR nix-lib
# for reusable build functions, Docker images, and formatting configuration.
#
# Structure:
# - nix/hose.nix: Crane-based Rust build with ReScript/protobuf integration
# - nix/rescript.nix: ReScript assets build (buildNpmPackage)
# - nix/packages/: Pre-commit hooks
# - nix/checks.nix: CI/CD quality checks
# - nix-lib (external): Docker images, dev shells, treefmt, and utilities

{
  description = "HOPR Session Explorer";

  inputs = {
    flake-parts.url = "github:hercules-ci/flake-parts";
    nixpkgs.url = "github:NixOS/nixpkgs/release-25.11";

    # HOPR Nix Library (provides Docker images, dev shells, treefmt)
    nix-lib.url = "github:hoprnet/nix-lib";

    # Rust build system
    crane.url = "github:ipetkov/crane";
    rust-overlay.url = "github:oxalica/rust-overlay";

    # Development tools and quality assurance
    pre-commit.url = "github:cachix/git-hooks.nix";
    flake-root.url = "github:srid/flake-root";

    # Input dependency optimization
    flake-parts.inputs.nixpkgs-lib.follows = "nixpkgs";
    pre-commit.inputs.nixpkgs.follows = "nixpkgs";
    nix-lib.inputs.nixpkgs.follows = "nixpkgs";
    nix-lib.inputs.crane.follows = "crane";
    nix-lib.inputs.rust-overlay.follows = "rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-parts,
      nix-lib,
      crane,
      rust-overlay,
      pre-commit,
      ...
    }@inputs:
    flake-parts.lib.mkFlake { inherit inputs; } {
      imports = [
        inputs.nix-lib.flakeModules.default
        inputs.flake-root.flakeModule
      ];

      perSystem =
        {
          config,
          lib,
          system,
          pkgs,
          ...
        }:
        let
          # Nixpkgs with rust-overlay
          overlays = [
            rust-overlay.overlays.default
          ];
          pkgs = import nixpkgs {
            inherit system overlays;
          };

          # Import nix-lib for this system
          nixLib = nix-lib.lib.${system};

          # Crane library for Rust builds
          craneLib = (crane.mkLib pkgs).overrideToolchain (
            p: p.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml
          );

          # ReScript assets (built via buildNpmPackage)
          rescriptAssets = import ./nix/rescript.nix { inherit pkgs; };

          # Hose builds using crane (release and dev profiles)
          hoseRelease = import ./nix/hose.nix {
            inherit pkgs craneLib rescriptAssets;
            profile = "release";
          };
          hoseDev = import ./nix/hose.nix {
            inherit pkgs craneLib rescriptAssets;
            profile = "dev";
          };

          # Container filesystem layout helper
          mkAppRoot =
            hosePkg:
            pkgs.runCommand "hose-root" { } ''
              mkdir -p $out/app $out/data $out/tmp
              cp ${hosePkg}/bin/hose $out/app/
              cp -r ${hosePkg}/share/hose/static $out/app/
              cp -r ${hosePkg}/share/hose/migrations $out/app/
              cp -r ${hosePkg}/share/hose/templates $out/app/
            '';

          # Docker images need Linux packages, even when building on macOS
          pkgsLinux = import nixpkgs {
            system = "x86_64-linux";
            inherit overlays;
          };

          craneLibLinux = (crane.mkLib pkgsLinux).overrideToolchain (
            p: p.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml
          );

          rescriptAssetsLinux = import ./nix/rescript.nix { pkgs = pkgsLinux; };

          hoseReleaseLinux = import ./nix/hose.nix {
            pkgs = pkgsLinux;
            craneLib = craneLibLinux;
            rescriptAssets = rescriptAssetsLinux;
            profile = "release";
          };
          hoseDevLinux = import ./nix/hose.nix {
            pkgs = pkgsLinux;
            craneLib = craneLibLinux;
            rescriptAssets = rescriptAssetsLinux;
            profile = "dev";
          };

          # Container environment variables
          containerEnv = [
            "HOSE_GRPC_LISTEN=0.0.0.0:4317"
            "HOSE_HTTP_LISTEN=0.0.0.0:8080"
            "HOSE_DATABASE_PATH=/data/hose.db"
            "HOSE_RETENTION_HOURS=24"
            "HOSE_WRITE_BUFFER_SIZE=1000"
            "HOSE_WRITE_BUFFER_FLUSH_SECS=5"
            "RUST_LOG=info"
            "SSL_CERT_FILE=${pkgsLinux.cacert}/etc/ssl/certs/ca-bundle.crt"
          ];

          # Docker images using nix-lib
          hoseDocker = {
            docker-hose-x86_64-linux = nixLib.mkDockerImage {
              name = "hose";
              Entrypoint = [
                "${mkAppRoot hoseReleaseLinux}/app/hose"
              ];
              pkgsLinux = pkgsLinux;
              env = containerEnv;
              extraContents = [
                (mkAppRoot hoseReleaseLinux)
                pkgsLinux.cacert
              ];
            };
            docker-hose-x86_64-linux-dev = nixLib.mkDockerImage {
              name = "hose-dev";
              Entrypoint = [
                "${mkAppRoot hoseDevLinux}/app/hose"
              ];
              pkgsLinux = pkgsLinux;
              env = containerEnv;
              extraContents = [
                (mkAppRoot hoseDevLinux)
                pkgsLinux.cacert
              ];
            };
          };

          dockerUploadApps = {
            docker-hose-upload-x86_64-linux = nixLib.mkDockerUploadApp hoseDocker.docker-hose-x86_64-linux;
            docker-hose-dev-upload-x86_64-linux = nixLib.mkDockerUploadApp hoseDocker.docker-hose-x86_64-linux-dev;
          };

          # Pre-commit hooks check
          preCommitCheck = pkgs.callPackage ./nix/packages/pre-commit-check.nix {
            inherit pre-commit system config;
          };

          # Rust toolchain for dev shell
          buildPlatform = pkgs.stdenv.buildPlatform;
          stableToolchain =
            (pkgs.pkgsBuildHost.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml).override
              {
                targets = [
                  (
                    if buildPlatform.config == "arm64-apple-darwin" then
                      "aarch64-apple-darwin"
                    else
                      buildPlatform.config
                  )
                ];
              };
        in
        {
          # Configure treefmt using nix-lib options
          nix-lib.treefmt = {
            globalExcludes = [
              # locally installed npm packages and build output
              ".npm/"
              "node_modules/"
              "lib/"

              # ReScript build output (copied into static/js/ by build)
              "static/js/"

              # Helm templates (contain Go template syntax that yamlfmt can't parse)
              "charts/hose/templates/*"

              # beads issue tracker
              ".beads/"
            ];
            extraFormatters = {
              # Markdown formatter
              settings.formatter.deno = {
                command = pkgs.writeShellApplication {
                  name = "deno-fmt";
                  runtimeInputs = [ pkgs.deno ];
                  text = ''
                    deno fmt --config deno.json "$@"
                  '';
                };
                includes = [
                  "**/*.md"
                  "*.md"
                ];
              };
              # GitHub Actions workflow linter
              settings.formatter.actionlint = {
                command = pkgs.writeShellApplication {
                  name = "actionlint";
                  runtimeInputs = [ pkgs.actionlint ];
                  text = ''
                    actionlint "$@"
                  '';
                };
                includes = [ ".github/workflows/*.yaml" ];
              };
              settings.formatter.yamlfmt.includes = [
                ".github/workflows/*.yaml"
              ];
              # Rust code linter using AST-based pattern matching
              settings.formatter.ast-grep = {
                command = pkgs.writeShellApplication {
                  name = "ast-grep-check";
                  runtimeInputs = [ pkgs.ast-grep ];
                  text = ''
                    ast-grep scan "$@"
                  '';
                };
                includes = [ "**/*.rs" ];
              };
            };
          };

          # Export checks for CI
          checks = import ./nix/checks.nix { inherit pkgs; };

          # Export applications
          apps = dockerUploadApps;

          # Export packages
          packages = {
            default = hoseRelease;
            hose = hoseRelease;
            hose-dev = hoseDev;
            rescript-assets = rescriptAssets;
            pre-commit-check = preCommitCheck;

            # Docker images
            inherit (hoseDocker)
              docker-hose-x86_64-linux
              docker-hose-x86_64-linux-dev
              ;
          };

          # Development shell
          devShells.default = nixLib.mkDevShell {
            rustToolchain = stableToolchain;
            shellName = "hose";
            treefmtWrapper = config.treefmt.build.wrapper;
            treefmtPrograms = pkgs.lib.attrValues config.treefmt.build.programs;
            shellHook = ''
              ${preCommitCheck.shellHook}

              export PROJECT_ROOT="$(${pkgs.git}/bin/git rev-parse --show-toplevel)"
              export PROTOC="${pkgs.protobuf}/bin/protoc"

              echo "HOSE dev environment loaded"
              echo "Rust $(rustc --version)"
              echo "Node $(node --version)"
            '';
            extraPackages = with pkgs; [
              protobuf
              pkg-config
              openssl
              sqlite
              treefmt
              deno
              nodejs_22
              just
              ast-grep
            ];
          };
        };

      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
        "x86_64-darwin"
      ];
    };
}
