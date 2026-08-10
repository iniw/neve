{
  self,
  lib,
  stdenvNoCC,
  postgres-language-server,
  postgresql,
  sqlx-cli,
}:
{
  postgres = stdenvNoCC.mkDerivation {
    name = "postgres";

    src = lib.sourceFilesBySuffices self [
      ".sql"
      "postgres-language-server.jsonc"
      "sqlx.toml"
    ];

    nativeBuildInputs = [
      postgres-language-server
      postgresql
      sqlx-cli
    ];

    buildPhase = ''
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

      postgres-language-server check . \
        --error-on-warnings \
        --max-diagnostics none

      postgres-language-server format . \
        --error-on-warnings \
        --max-diagnostics none

      postgres-language-server dblint \
        --error-on-warnings \
        --max-diagnostics none

      pg_ctl stop
      trap - EXIT

      touch $out
    '';
  };
}
