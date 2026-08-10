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

      mkDevShells =
        system:
        let
          pkgs = inputs.nixpkgs.legacyPackages.${system};
        in
        pkgs.callPackages ./nix/devShells.nix {
          checks = inputs.self.checks.${system};
        };

      mkCheckGroups =
        system:
        let
          pkgs = inputs.nixpkgs.legacyPackages.${system};
          crane = inputs.crane.mkLib pkgs;
        in
        import ./nix/checks {
          inherit (inputs) self;
          inherit pkgs crane;
        };
    in
    {
      devShells = forAllSystems (system: mkDevShells system);

      checks = forAllSystems (system: mkCheckGroups system |> lib.attrValues |> lib.mergeAttrsList);

      ci = {
        # Linux CI runs all checks from all groups.
        inherit (inputs.self.checks) x86_64-linux;

        # Other platforms run only the "cargo" group's checks, to make sure the code still works.
        aarch64-darwin = mkCheckGroups "aarch64-darwin" |> lib.getAttr "cargo";
        aarch64-linux = mkCheckGroups "aarch64-linux" |> lib.getAttr "cargo";
      };

      # Allow easily running a specific check with `nix build .#foo`
      packages = inputs.self.checks;
    };
}
