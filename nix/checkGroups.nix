{
  self,
  pkgs,
  crane,
}:
let
  # Group for cargo checks that may take several minutes to complete without a cache.
  cargo = import ./checks/cargo.nix { inherit self pkgs crane; };

  # Group for lint checks that don't need their own runner.
  lint = import ./checks/lint.nix { inherit self pkgs; };

  withGroup =
    groupName: checks:
    checks
    |> pkgs.lib.mapAttrs (
      _: check:
      check.overrideAttrs {
        meta.hestia.group = "${groupName} @ ${pkgs.stdenv.hostPlatform.system}";
      }
    );
in
{
  cargo = cargo |> withGroup "Cargo checks";
  lint = lint |> withGroup "Lint checks";
}
