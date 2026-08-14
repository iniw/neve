{
  lib,
  callPackages,
  stdenvNoCC,
}:
{
  # Cargo workspace checks.
  cargo = callPackages ./cargo.nix { };

  # Postgres migrations checks.
  postgres = callPackages ./postgres.nix { };

  # Miscellaneous linting checks.
  lint = callPackages ./lint.nix { };
}
|> lib.mapAttrs (
  group: checks:
  checks
  |> lib.mapAttrs (
    _:
    lib.addMetaAttrs {
      hestia.group = "${lib.toSentenceCase group} checks @ ${stdenvNoCC.hostPlatform.system}";
    }
  )
)
