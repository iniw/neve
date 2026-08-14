ephemeralPostgresDbHookStart() {
  export PGDATA="$TMPDIR/postgres"
  export PGDATABASE=neve
  export PGHOST="$TMPDIR"
  export PGUSER=postgres

  initdb \
    --encoding UTF8 \
    --username "$PGUSER"

  pg_ctl start \
    --wait \
    --options "-c listen_addresses= -k $PGHOST"

  failureHooks+=(ephemeralPostgresDbHookStop)

  # Percent-encode the Unix socket path for SQLx's connection URL.
  local encoded_socket="${PGHOST//\//%2F}"
  export DATABASE_URL="postgres://$encoded_socket"

  sqlx database setup
}

ephemeralPostgresDbHookStop() {
  pg_ctl stop
  failureHooks=("${failureHooks[@]/ephemeralPostgresDbHookStop}")
}

preBuildHooks+=(ephemeralPostgresDbHookStart)
postInstallHooks+=(ephemeralPostgresDbHookStop)
