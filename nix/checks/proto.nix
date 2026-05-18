{
  self,
  lib,
  stdenvNoCC,
  buf,
}:
let
  buf-cmd =
    cmd: name:
    stdenvNoCC.mkDerivation {
      name = "proto-${name}";

      src = lib.sourceFilesBySuffices self [
        "buf.yaml"
        ".proto"
      ];

      nativeBuildInputs = [ buf ];

      buildPhase = ''
        # buf requires a valid $HOME, otherwise it fails with:
        # mkdir /homeless-shelter: operation not permitted
        export HOME=$(mktemp -d)

        buf ${cmd}

        touch $out
      '';
    };
in
{
  proto-lint = buf-cmd "lint" "lint";
  proto-fmt = buf-cmd "format --exit-code --diff" "fmt";
}
