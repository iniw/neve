{
  checks,
  lib,
  mkShell,

  # Rust devtools
  rust-analyzer,

  # Web client devtools
  deno,
  svelte-language-server,
  vscode-langservers-extracted,
}:
{
  default = mkShell {
    inputsFrom = lib.attrValues checks;

    packages = [
      # Rust devtools
      rust-analyzer

      # Web client devtools
      deno
      svelte-language-server
      vscode-langservers-extracted
    ];
  };
}
