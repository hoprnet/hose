{ pkgs }:

pkgs.buildNpmPackage {
  pname = "hose-rescript";
  version = "0.1.0";

  src = let
    fs = pkgs.lib.fileset;
    root = ./..;
  in
    fs.toSource {
      inherit root;
      fileset = fs.unions [
        ./../package.json
        ./../package-lock.json
        ./../rescript.json
        ./../rescript
      ];
    };

  npmDepsHash = "sha256-+Lsc9LtD4pwE5G7ltl4qJx3mSKdDHbLy0mc7E+IwUII=";

  # We handle the build ourselves since rescript has a custom build step
  dontNpmBuild = true;

  buildPhase = ''
    runHook preBuild
    npm run res:build
    runHook postBuild
  '';

  installPhase = ''
    runHook preInstall
    mkdir -p $out/static/js $out/static/js/rescript
    cp lib/es6/rescript/src/*.mjs $out/static/js/
    cp node_modules/rescript/lib/es6/js_dict.js \
       node_modules/rescript/lib/es6/js_json.js \
       node_modules/rescript/lib/es6/js_promise.js \
       node_modules/rescript/lib/es6/caml_option.js \
       node_modules/rescript/lib/es6/curry.js \
       node_modules/rescript/lib/es6/caml_array.js \
       $out/static/js/rescript/
    runHook postInstall
  '';
}
