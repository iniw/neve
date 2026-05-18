{
  self,
  lib,
  stdenvNoCC,
  tombi,
}:
let
  tombi-cmd =
    cmd: name:
    stdenvNoCC.mkDerivation {
      name = "toml-${name}";

      src = lib.sourceFilesBySuffices self [ ".toml" ];

      nativeBuildInputs = [ tombi ];

      buildPhase = ''
        tombi ${cmd}

        touch $out
      '';
    };
in
{
  toml-lint = tombi-cmd "lint --offline" "lint";
  toml-fmt = tombi-cmd "format --offline --check --diff" "fmt";
}
