{
  pkgs,
  crane,
  checks,
}:
{
  default = crane.devShell {
    inherit checks;

    packages = with pkgs; [
      rust-analyzer
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
