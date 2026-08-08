{
  self,
  pkgs,
  crane,
}:
let
  # Checks that build, test and lint the cargo workspace.
  cargo = import ./checks/cargo.nix { inherit self pkgs crane; };

  # Miscellaneous lint checks.
  lint = import ./checks/lint.nix { inherit self pkgs; };

  # Postgres linting checks that run against an ephemeral database.
  postgres = import ./checks/postgres.nix { inherit self pkgs; };

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
  postgres = postgres |> withGroup "Postgres checks";
}
