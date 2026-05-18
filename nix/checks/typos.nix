{
  self,
  stdenvNoCC,
  typos,
}:
stdenvNoCC.mkDerivation {
  name = "typos-check";

  src = self;

  nativeBuildInputs = [ typos ];

  buildPhase = ''
    typos

    touch $out
  '';
}
