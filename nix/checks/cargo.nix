{
  lib,
  crane,
  protobuf,
}:
let
  root = ../../.;

  src = lib.fileset.toSource {
    inherit root;
    fileset = lib.fileset.unions [
      (crane.fileset.commonCargoSources root)
      (lib.fileset.fileFilter (file: file.hasExt "proto") root)
    ];
  };

  cargoArtifacts = crane.buildDepsOnly {
    inherit src;
    strictDeps = true;
  };

  nativeBuildInputs = [ protobuf ];
in
rec {
  cargo-build = crane.buildPackage {
    inherit src nativeBuildInputs cargoArtifacts;

    cargoBuildExtraArgs = "--all-targets";

    # We'll run tests in another check
    doCheck = false;

    # Install `target/` as an output so that the `cargo-test` check has a cached build
    doInstallCargoArtifacts = true;
  };

  cargo-test = crane.cargoTest {
    inherit src;

    cargoArtifacts = cargo-build;
  };

  cargo-clippy = crane.cargoClippy {
    inherit src nativeBuildInputs cargoArtifacts;

    cargoClippyExtraArgs = "--all-targets -- --deny warnings";
  };

  cargo-fmt = crane.cargoFmt {
    inherit src;
  };

  cargo-doc = crane.cargoDoc {
    inherit src nativeBuildInputs cargoArtifacts;
  };
}
