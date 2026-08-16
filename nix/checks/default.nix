{
  pkgs,
  lib,
  callPackage,
}:
let
  pkgs' = pkgs.extend (
    final: prev: {
      ephemeralPostgresDbHook = callPackage ./ephemeralPostgresDbHook { };
    }
  );

  groups =
    pkgs'.callPackage ./groups { }
    # Using `callPackage` adds extra callable attributes (`override` et al.) that shouldn't be presented as a group.
    |> lib.filterAttrs (_: group: !lib.isFunction group);
in
groups
// {
  combined = groups |> lib.attrValues |> lib.mergeAttrsList;
}
