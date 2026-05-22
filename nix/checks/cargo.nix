{
  self,
  crane,
  lib,
  protobuf,
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
    filter = path: type: (crane.filterCargoSources path type) || lib.hasSuffix ".proto" path;
  };

  nativeBuildInputs = [ protobuf ];

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
      nativeBuildInputs
      cargoArtifacts
      ;

    cargoBuildExtraArgs = "--all-targets";

    # We want to make sure the code builds and runs properly on all CI-tested platforms.
    passthru.multiPlatform = true;
  };

  cargo-clippy = crane.cargoClippy {
    inherit
      pname
      src
      nativeBuildInputs
      cargoArtifacts
      ;

    cargoClippyExtraArgs = "--all-targets -- --deny warnings";
  };

  cargo-fmt = crane.cargoFmt {
    inherit pname src;
  };

  cargo-doc = crane.cargoDoc {
    inherit
      pname
      src
      nativeBuildInputs
      cargoArtifacts
      ;

    # Rust doesn't offer a nice CLI interface to deny warnings from `cargo doc`.
    # See: https://github.com/rust-lang/cargo/issues/8424#issuecomment-1070988443
    env.RUSTDOCFLAGS = "--deny warnings";
  };
}
