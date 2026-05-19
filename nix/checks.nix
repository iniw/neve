{
  self,
  pkgs,
  crane,
}:
let
  cargo-checks = pkgs.callPackage ./checks/cargo.nix { inherit self crane; };
  proto-checks = pkgs.callPackage ./checks/proto.nix { inherit self; };
  toml-checks = pkgs.callPackage ./checks/toml.nix { inherit self; };
  misc-checks = pkgs.callPackage ./checks/misc.nix { inherit self; };
in
{
  inherit (cargo-checks)
    cargo-build-and-test
    cargo-clippy
    cargo-fmt
    cargo-doc
    ;

  inherit (proto-checks) proto-lint proto-fmt;

  inherit (toml-checks) toml-lint toml-fmt;

  inherit (misc-checks) actionlint typos;
}
