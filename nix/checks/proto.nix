{
  self,
  lib,
  stdenvNoCC,
  buf,
}:
let
  run-buf =
    { name, cmd }:
    stdenvNoCC.mkDerivation {
      inherit name;

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
  proto-lint = run-buf {
    name = "proto-lint";
    cmd = "lint";
  };

  proto-fmt = run-buf {
    name = "proto-fmt";
    cmd = "format --exit-code --diff";
  };
}
