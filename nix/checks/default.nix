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

  groups =
    pkgs'.callPackage ./groups { }
    # Using `callPackage` adds extra callable attributes alongside the check groups that we don't care about.
    |> lib.filterAttrs (_: group: !lib.isFunction group);
in
groups
// {
  combined = groups |> lib.attrValues |> lib.mergeAttrsList;
}
