{
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
      in
      {
        devShells.default = crane.devShell {
          checks = inputs.self.checks.${system};

          packages = with pkgs; [
            rust-analyzer
          ];
        };

        checks =
          let
            src = crane.cleanCargoSource ./.;

            cargoArtifacts = crane.buildDepsOnly {
              inherit src;
              strictDeps = true;
            };
          in
          {
            build-and-test = crane.buildPackage {
              inherit src cargoArtifacts;
            };

            clippy = crane.cargoClippy {
              inherit src cargoArtifacts;

              cargoClippyExtraArgs = "--all-targets -- --deny warnings";
            };

            doc = crane.cargoDocTest {
              inherit src cargoArtifacts;
            };

            fmt = crane.cargoFmt {
              inherit src;
            };

            toml-fmt = pkgs.stdenvNoCC.mkDerivation {
              name = "toml-fmt";
              src = pkgs.lib.sources.sourceFilesBySuffices ./. [ ".toml" ];

              nativeBuildInputs = [ pkgs.tombi ];

              buildPhase = ''
                tombi format --check --diff --offline
                tombi lint --offline
                touch $out
              '';
            };

            typos = pkgs.stdenvNoCC.mkDerivation {
              name = "typos";
              src = ./.;

              nativeBuildInputs = [ pkgs.typos ];

              buildPhase = ''
                typos
                touch $out
              '';
            };
          };
      }
    );
}
