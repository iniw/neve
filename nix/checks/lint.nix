{
  self,
  lib,
  stdenvNoCC,
  actionlint,
  buf,
  tombi,
  typos,
}:
{
  actionlint = stdenvNoCC.mkDerivation {
    name = "actionlint";

    src = lib.cleanSourceWith {
      src = self;
      filter = path: _: lib.hasPrefix "${self}/.github" path;
    };

    nativeBuildInputs = [ actionlint ];

    buildPhase = ''
      # actionlint requires a git repository by checking for the existence of a .git folder.
      mkdir .git

      actionlint

      touch $out
    '';
  };

  proto = stdenvNoCC.mkDerivation {
    name = "proto";

    src = lib.sourceFilesBySuffices self [
      ".proto"
      "buf.yaml"
    ];

    nativeBuildInputs = [ buf ];

    buildPhase = ''
      # buf requires a valid $HOME, otherwise it fails with:
      # mkdir /homeless-shelter: operation not permitted
      export HOME=$(mktemp -d)

      buf lint
      buf format --exit-code --diff

      touch $out
    '';
  };

  toml = stdenvNoCC.mkDerivation {
    name = "toml";

    src = lib.sourceFilesBySuffices self [ ".toml" ];

    nativeBuildInputs = [ tombi ];

    buildPhase = ''
      tombi lint --offline --error-on-warnings
      tombi format --offline --check --diff

      touch $out
    '';
  };

  typos = stdenvNoCC.mkDerivation {
    name = "typos";

    src = self;

    nativeBuildInputs = [ typos ];

    buildPhase = ''
      typos --diff --sort

      touch $out
    '';
  };
}
