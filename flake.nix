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

      checkGroupsFor =
        system:
        let
          pkgs = inputs.nixpkgs.legacyPackages.${system};
          crane = inputs.crane.mkLib pkgs;
        in
        import ./nix/checkGroups.nix {
          inherit (inputs) self;
          inherit pkgs crane;
        };
    in
    {
      devShells = forAllSystems (
        system:
        let
          pkgs = inputs.nixpkgs.legacyPackages.${system};
          crane = inputs.crane.mkLib pkgs;
        in
        import ./nix/devShells.nix {
          inherit pkgs crane;
          checks = inputs.self.checks.${system};
        }
      );

      checks = forAllSystems (system: checkGroupsFor system |> lib.attrValues |> lib.mergeAttrsList);

      ci = {
        # Linux CI runs all checks from all groups.
        x86-64_linux = checkGroupsFor "x86_64-linux" |> lib.attrValues |> lib.mergeAttrsList;

        # Other platforms run only the "cargo" group's checks, to make sure the code still works.
        aarch64-darwin = checkGroupsFor "aarch64-darwin" |> lib.getAttr "cargo";
        aarch64-linux = checkGroupsFor "aarch64-linux" |> lib.getAttr "cargo";
      };

      # Allow easily running a specific check with `nix build .#foo`
      packages = inputs.self.checks;
    };
}
