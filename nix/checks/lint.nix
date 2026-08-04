{
  self,
  pkgs,
}:
{
  actionlint = pkgs.stdenvNoCC.mkDerivation {
    name = "actionlint";

    src = pkgs.lib.cleanSourceWith {
      src = self;
      filter = path: _: pkgs.lib.hasPrefix "${self}/.github" path;
    };

    nativeBuildInputs = [ pkgs.actionlint ];

    buildPhase = ''
      # actionlint requires a git repository by checking for the existence of a .git folder.
      mkdir .git

      actionlint

      touch $out
    '';
  };

  proto = pkgs.stdenvNoCC.mkDerivation {
    name = "proto";

    src = pkgs.lib.sourceFilesBySuffices self [
      "buf.yaml"
      ".proto"
    ];

    nativeBuildInputs = [ pkgs.buf ];

    buildPhase = ''
      # buf requires a valid $HOME, otherwise it fails with:
      # mkdir /homeless-shelter: operation not permitted
      export HOME=$(mktemp -d)

      buf lint
      buf format --exit-code --diff

      touch $out
    '';
  };

  toml = pkgs.stdenvNoCC.mkDerivation {
    name = "toml";

    src = pkgs.lib.sourceFilesBySuffices self [ ".toml" ];

    nativeBuildInputs = [ pkgs.tombi ];

    buildPhase = ''
      tombi lint --offline --error-on-warnings
      tombi format --offline --check --diff

      touch $out
    '';
  };

  typos = pkgs.stdenvNoCC.mkDerivation {
    name = "typos";

    src = self;

    nativeBuildInputs = [ pkgs.typos ];

    buildPhase = ''
      typos --diff --sort

      touch $out
    '';
  };
}
