{
  pkgs,
  crane,
}:
let
  cargo-checks = pkgs.callPackage ./checks/cargo.nix { inherit crane; };
  proto-checks = pkgs.callPackage ./checks/proto.nix { };
  toml-checks = pkgs.callPackage ./checks/toml.nix { };
  typos-check = pkgs.callPackage ./checks/typos.nix { };
in
{
  inherit (cargo-checks)
    cargo-build
    cargo-test
    cargo-clippy
    cargo-fmt
    cargo-doc
    ;

  inherit (proto-checks) proto-lint proto-fmt;

  inherit (toml-checks) toml-lint toml-fmt;

  inherit typos-check;
}
