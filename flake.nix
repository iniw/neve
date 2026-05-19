{
  nixConfig = {
    extra-substituters = [ "https://neve.cachix.org" ];
    extra-trusted-public-keys = [ "neve.cachix.org-1:41XWH1l3h3QGtKzDMlOCrXGD1B7uf55fRqcGtOg7tLU=" ];
  };

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    crane.url = "github:ipetkov/crane";
  };

  outputs =
    inputs:
    inputs.flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import inputs.nixpkgs { inherit system; };
        crane = inputs.crane.mkLib pkgs;

        checks = import ./nix/checks.nix {
          inherit (inputs) self;
          inherit pkgs crane;
        };
        devShells = import ./nix/devshells.nix { inherit pkgs crane checks; };
      in
      {
        inherit checks devShells;

        # Allow easily running a specific check with `nix build .#foo`
        packages = checks;
      }
    );
}
