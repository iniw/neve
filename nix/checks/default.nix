{
  self,
  pkgs,
  crane,
}:
let
  # Checks that build, test and lint the cargo workspace.
  cargo = pkgs.callPackages ./cargo.nix { inherit self crane; };

  # Miscellaneous lint checks.
  lint = pkgs.callPackages ./lint.nix { inherit self; };

  # Postgres linting checks that run against an ephemeral database.
  postgres = pkgs.callPackages ./postgres.nix { inherit self; };

  withGroup =
    groupName: checks:
    checks
    |> pkgs.lib.mapAttrs (
      _: check:
      check.overrideAttrs {
        meta.hestia.group = "${groupName} @ ${pkgs.stdenvNoCC.hostPlatform.system}";
      }
    );
in
{
  cargo = cargo |> withGroup "Cargo checks";
  lint = lint |> withGroup "Lint checks";
  postgres = postgres |> withGroup "Postgres checks";
}
