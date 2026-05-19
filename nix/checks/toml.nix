{
  self,
  lib,
  stdenvNoCC,
  tombi,
}:
let
  run-tombi =
    { name, cmd }:
    stdenvNoCC.mkDerivation {
      inherit name;

      src = lib.sourceFilesBySuffices self [ ".toml" ];

      nativeBuildInputs = [ tombi ];

      buildPhase = ''
        tombi ${cmd}

        touch $out
      '';
    };
in
{
  toml-lint = run-tombi {
    name = "toml-lint";
    cmd = "lint --offline";
  };

  toml-fmt = run-tombi {
    name = "toml-fmt";
    cmd = "format --offline --check --diff";
  };
}
