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

      checksGroupsFor =
        system:
        let
          checksGroups = import ./nix/checksGroups.nix;

          pkgs = inputs.nixpkgs.legacyPackages.${system};
          crane = inputs.crane.mkLib pkgs;
        in
        checksGroups {
          inherit (inputs) self;
          inherit pkgs crane;
        };
    in
    {
      devShells = forAllSystems (
        system:
        let
          devShells = import ./nix/devShells.nix;

          pkgs = inputs.nixpkgs.legacyPackages.${system};
          crane = inputs.crane.mkLib pkgs;
        in
        devShells {
          inherit pkgs crane;
          checks = inputs.self.checks.${system};
        }
      );

      checks = forAllSystems (system: checksGroupsFor system |> lib.attrValues |> lib.mergeAttrsList);

      ci = {
        # Linux CI runs all checks from all groups.
        x86-64_linux = checksGroupsFor "x86_64-linux" |> lib.attrValues |> lib.mergeAttrsList;

        # Darwin CI runs only the "cargo" group's checks.
        aarch64-darwin = checksGroupsFor "aarch64-darwin" |> lib.getAttr "cargo";
      };

      # Allow easily running a specific check with `nix build .#foo`
      packages = inputs.self.checks;
    };
}
