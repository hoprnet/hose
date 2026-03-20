{
  pkgs,
  craneLib,
  profile ? "release",
  rescriptAssets,
}:

let
  unfilteredRoot = ./..;

  src = pkgs.lib.fileset.toSource {
    root = unfilteredRoot;
    fileset = pkgs.lib.fileset.unions [
      (craneLib.fileset.commonCargoSources unfilteredRoot)
      (pkgs.lib.fileset.maybeMissing ./../templates)
      (pkgs.lib.fileset.maybeMissing ./../migrations)
      (pkgs.lib.fileset.maybeMissing ./../static/css)
    ];
  };

  # crane uses CARGO_PROFILE to control the build profile.
  # "release" produces an optimized binary; "" omits the flag (defaults to dev).
  cargoProfile = if profile == "release" then "release" else "";

  commonArgs = {
    inherit src;
    pname = "hose";
    version = "0.1.0";
    strictDeps = true;

    CARGO_PROFILE = cargoProfile;

    nativeBuildInputs = with pkgs; [
      protobuf
      pkg-config
    ];
    buildInputs = with pkgs; [
      openssl
      sqlite
    ];

    # Merge ReScript-built JS assets into static/
    preBuild = ''
      mkdir -p static/js
      cp -r ${rescriptAssets}/static/js/* static/js/
    '';

    PROTOC = "${pkgs.protobuf}/bin/protoc";
  };

  cargoArtifacts = craneLib.buildDepsOnly commonArgs;
in
craneLib.buildPackage (
  commonArgs
  // {
    inherit cargoArtifacts;

    postInstall = ''
      mkdir -p $out/share/hose
      cp -r static $out/share/hose/
      cp -r migrations $out/share/hose/
      cp -r templates $out/share/hose/
      # Ensure all copied files are writable so crane's strip-references hook
      # can process them without "Permission denied" on read-only Nix store files.
      chmod -R u+w $out/share/hose/
    '';
  }
)
