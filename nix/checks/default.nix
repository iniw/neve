{
  lib,
  extend,
  callPackage,
}:
let
  ephemeralPostgresDbHook = callPackage ./ephemeralPostgresDbHook { };

  pkgs' = extend (
    final: prev: {
      inherit ephemeralPostgresDbHook;
    }
  );

  groups = pkgs'.callPackage ./groups { };
in
groups
// {
  combined =
    groups
    # Using `callPackage` adds `override` functions alongside the check groups
    # that we don't want to be part of the combined list of checks.
    |> lib.filterAttrs (_: group: !lib.isFunction group)
    |> lib.attrValues
    |> lib.mergeAttrsList;
}
