{
  self,
  crane,
  lib,
  protobuf,
  ephemeralPostgresDbHook,
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
  # The only exception to this is the `crane.buildPackage` builder that we use for the `cargo-build-and-test` check,
  # which doesn't append a suffix, so we just set it's name manually.
  pname = "cargo";

  src = lib.cleanSourceWith {
    src = self;
    filter =
      path: type:
      crane.filterCargoSources path type || lib.hasSuffix ".proto" path || lib.hasSuffix ".sql" path;
  };

  env = {
    CARGO_BUILD_WARNINGS = "deny";
    RUST_BACKTRACE = "1";
  };

  nativeBuildInputs = [
    # prost requires protoc to compile the protobuf files.
    protobuf
    # SQLx needs a live database to analyze queries and run tests.
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

    cargoClippyExtraArgs = "--all-targets --all-features";
  };

  cargo-doc = crane.cargoDoc {
    inherit
      pname
      src
      env
      nativeBuildInputs
      cargoArtifacts
      ;
  };

  cargo-fmt = crane.cargoFmt {
    inherit pname src;
  };
}
