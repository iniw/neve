{
  self,
  lib,
  stdenvNoCC,
  ephemeralPostgresDbHook,
  postgres-language-server,
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
      ephemeralPostgresDbHook
      postgres-language-server
    ];

    buildPhase = ''
      runHook preBuild

      postgres-language-server check . \
        --error-on-warnings \
        --max-diagnostics none

      postgres-language-server format . \
        --error-on-warnings \
        --max-diagnostics none

      postgres-language-server dblint \
        --error-on-warnings \
        --max-diagnostics none

      touch $out

      runHook postBuild
    '';
  };
}
