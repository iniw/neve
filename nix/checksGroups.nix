{
  self,
  pkgs,
  crane,
}:
let
  inherit (pkgs) lib;

  cargo = pkgs.callPackage ./checks/cargo.nix { inherit self crane; };

  other =
    let
      proto = pkgs.callPackage ./checks/proto.nix { inherit self; };
      toml = pkgs.callPackage ./checks/toml.nix { inherit self; };
      misc = pkgs.callPackage ./checks/misc.nix { inherit self; };
    in
    [
      proto
      toml
      misc
    ]
    |> lib.mergeAttrsList;

  group =
    { checks, name, ... }:
    # `callPackage` adds non-derivation helpers such as `override` and `overrideDerivation` to returned attrsets.
    checks
    |> lib.filterAttrs (_: lib.isDerivation)
    |> lib.mapAttrs (
      _: check:
      check.overrideAttrs {
        meta.hestia.group = "${name} @ ${pkgs.stdenv.hostPlatform.system}";
      }
    );
in
{
  cargo = group {
    checks = cargo;
    name = "Cargo checks";
  };

  other = group {
    checks = other;
    name = "Other checks";
  };
}
