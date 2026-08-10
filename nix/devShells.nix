{
  checks,
  lib,
  mkShell,
  cargo,
  clippy,
  rust-analyzer,
  rustc,
  rustfmt,
}:
{
  default = mkShell {
    inputsFrom = lib.attrValues checks;

    packages = [
      cargo
      clippy
      rust-analyzer
      rustc
      rustfmt
    ];

    # Setup the DB with:
    # pg_ctl initdb
    # pg_ctl start --log $PGDATA/pg.log
    # sqlx database setup
    shellHook = ''
      export PGDATA="server/db"
      export PGDATABASE="neve"
      export PGUSER="$USER"
      export DATABASE_URL="postgres://"
    '';
  };
}
