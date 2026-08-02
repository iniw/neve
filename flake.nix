{
  nixConfig = {
    extra-substituters = [ "https://neve.cachix.org" ];
    extra-trusted-public-keys = [ "neve.cachix.org-1:41XWH1l3h3QGtKzDMlOCrXGD1B7uf55fRqcGtOg7tLU=" ];
  };

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    crane.url = "github:ipetkov/crane/v0.23.4";
  };

  outputs =
    inputs:
    let
      inherit (inputs.nixpkgs) lib;

      forAllSystems =
        f:
        lib.genAttrs lib.systems.flakeExposed (
          system:
          let
            pkgs = inputs.nixpkgs.legacyPackages.${system};
            crane = inputs.crane.mkLib pkgs;
          in
          f { inherit system pkgs crane; }
        );

      devShells = import ./nix/devShells.nix;
      checks = import ./nix/checks.nix;
    in
    {
      devShells = forAllSystems (
        ctx:
        devShells {
          inherit (ctx) pkgs crane;
          checks = inputs.self.checks.${ctx.system};
        }
      );

      checks = forAllSystems (
        ctx:
        checks {
          inherit (inputs) self;
          inherit (ctx) pkgs crane;
        }
      );

      # Allow easily running a specific check with `nix build .#foo`
      packages = inputs.self.checks;
    };
}
