{
  makeSetupHook,
  postgresql,
  sqlx-cli,
}:
makeSetupHook {
  name = "ephemeral-postgres-db-hook";
  propagatedBuildInputs = [
    postgresql
    sqlx-cli
  ];
} ./hook.sh
