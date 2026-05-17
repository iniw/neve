{
  stdenvNoCC,
  typos,
}:
stdenvNoCC.mkDerivation {
  name = "typos-check";

  src = ../../.;

  nativeBuildInputs = [ typos ];

  buildPhase = ''
    typos

    touch $out
  '';
}
