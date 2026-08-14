{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    crane.url = "github:ipetkov/crane/v0.23.4";
  };

  outputs =
    inputs:
    let
      inherit (inputs.nixpkgs) lib;

      forAllSystems = f: lib.genAttrs lib.systems.flakeExposed (system: f system);

      checkGroups = forAllSystems (
        system:
        let
          pkgs = inputs.nixpkgs.legacyPackages.${system}.extend (
            final: prev: {
              inherit (inputs) self;
              crane = inputs.crane.mkLib final;
            }
          );
        in
        pkgs.callPackage ./nix/checks { }
      );
    in
    {
      checks = forAllSystems (system: checkGroups.${system}.combined);

      ci = lib.mapAttrs (system: group: checkGroups.${system}.${group}) {
        x86_64-linux = "combined";
        aarch64-darwin = "cargo";
        aarch64-linux = "cargo";
      };

      devShells = forAllSystems (
        system:
        let
          pkgs = inputs.nixpkgs.legacyPackages.${system};
        in
        pkgs.callPackages ./nix/devShells.nix {
          checks = inputs.self.checks.${system};
        }
      );

      # To easily run a specific check with `nix build .#foo`
      packages = inputs.self.checks;
    };
}
