{
  self,
  crane,
  lib,
  ephemeralPostgresDbHook,
  protobufGenerationHook,
  cargo-shear,
}:
let
  # crane automatically appends the "type" of the derivation to as a suffix `pname`:
  # "-test" for `crane.cargoTest`, "-clippy" for `crane.cargoClippy`, "-doc" for `crane.cargoDoc`, ...
  #
  # We want to keep the derivations consistent with their check's name:
  # "cargo-clippy", "cargo-fmt", "cargo-doc"...
  #
  # To achieve this we set `pname` to "cargo" and let crane add the suffix, then match the check's name to crane's
  # suffix.
  # Builders without a matching suffix set `pname` manually.
  pname = "cargo";

  src = lib.cleanSourceWith {
    src = self;
    filter =
      path: type:
      crane.filterCargoSources path type
      || (lib.any (suffix: lib.hasSuffix suffix path) [
        ".proto"
        ".sql"
        "buf.gen.yaml"
        "buf.yaml"
      ]);
  };

  env = {
    CARGO_BUILD_WARNINGS = "deny";
    RUST_BACKTRACE = "1";
  };

  nativeBuildInputs = [
    # Generate the Rust bindings before Cargo reads the protobuf crate.
    protobufGenerationHook
    # sqlx needs a live database to analyze queries and run tests.
    ephemeralPostgresDbHook
  ];

  # For `crane.buildDepsOnly` crane adds "-deps" to `pname`, so the derivation is called "cargo-deps".
  cargoArtifacts = crane.buildDepsOnly {
    inherit pname src;
  };
in
{
  cargo-build-and-test = crane.buildPackage {
    pname = "cargo-build-and-test";

    inherit
      src
      env
      nativeBuildInputs
      cargoArtifacts
      ;
  };

  cargo-clippy = crane.cargoClippy {
    inherit
      pname
      src
      env
      nativeBuildInputs
      cargoArtifacts
      ;
  };

  cargo-doc = crane.cargoDoc {
    inherit
      pname
      src
      env
      nativeBuildInputs
      cargoArtifacts
      ;

    cargoDocExtraArgs = "--no-deps --bins";
  };

  cargo-fmt = crane.cargoFmt {
    inherit pname src;
  };

  cargo-shear = crane.mkCargoDerivation {
    pname = "cargo-shear";

    inherit src;

    nativeBuildInputs = [ cargo-shear ];
    buildPhaseCargoCommand = "cargo shear --frozen --deny-warnings";

    # Note that this and cargo-deps still use the same vendor derivation, so both checks can run in parallel after
    # vendoring the dependencies, which is optimal in terms of work distribution.
    cargoArtifacts = null;
    # cargo shear does not build artifacts, so there are no new artifacts to save.
    doInstallCargoArtifacts = false;

  };
}
