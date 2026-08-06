{
  self,
  pkgs,
  crane,
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

  src = pkgs.lib.cleanSourceWith {
    src = self;
    filter =
      path: type:
      crane.filterCargoSources path type
      || pkgs.lib.hasSuffix ".proto" path
      || pkgs.lib.hasSuffix ".sql" path;
  };

  nativeBuildInputs = [ pkgs.protobuf ];

  withPostgres =
    check:
    check.overrideAttrs (prevAttrs: {
      nativeBuildInputs =
        with pkgs;
        [
          postgresql
          sqlx-cli
        ]
        ++ prevAttrs.nativeBuildInputs;

      preBuild = ''
        export PGDATA="$TMPDIR/postgres"
        export PGDATABASE=neve
        export PGUSER=postgres

        socket="$TMPDIR"

        initdb \
          --encoding UTF8 \
          --username "$PGUSER"

        pg_ctl start \
          --wait \
          --options "-c listen_addresses= -k $socket"

        trap 'pg_ctl stop --mode immediate || true' EXIT

        # Percent-encode the Unix socket path for SQLx's connection URL.
        encoded_socket="''${socket//\//%2F}"
        export DATABASE_URL="postgres://$encoded_socket"

        sqlx database setup
      '';

      postInstall = ''
        pg_ctl stop
        trap - EXIT
      '';
    });

  # For `crane.buildDepsOnly` crane adds "-deps" to `pname`, so the derivation is called "cargo-deps".
  cargoArtifacts = crane.buildDepsOnly {
    inherit pname src;
  };
in
{
  cargo-build-and-test =
    crane.buildPackage {
      pname = "cargo-build-and-test";

      inherit src nativeBuildInputs cargoArtifacts;

      cargoBuildExtraArgs = "--all-targets";

      cargoTestExtraArgs = "--no-fail-fast";

      env.RUST_BACKTRACE = "1";
    }
    |> withPostgres;

  cargo-clippy =
    crane.cargoClippy {
      inherit
        pname
        src
        nativeBuildInputs
        cargoArtifacts
        ;

      cargoClippyExtraArgs = "--all-targets --all-features -- --deny warnings";
    }
    |> withPostgres;

  cargo-doc =
    crane.cargoDoc {
      inherit
        pname
        src
        nativeBuildInputs
        cargoArtifacts
        ;

      # Rust doesn't offer a nice CLI interface to deny warnings from `cargo doc`.
      # See: https://github.com/rust-lang/cargo/issues/8424#issuecomment-1070988443
      env.RUSTDOCFLAGS = "--deny warnings";
    }
    |> withPostgres;

  cargo-fmt = crane.cargoFmt {
    inherit pname src;
  };
}
