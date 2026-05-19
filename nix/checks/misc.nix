{
  self,
  lib,
  stdenvNoCC,
  actionlint,
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

  typos = stdenvNoCC.mkDerivation {
    name = "typos";

    src = self;

    nativeBuildInputs = [ typos ];

    buildPhase = ''
      typos

      touch $out
    '';
  };
}
