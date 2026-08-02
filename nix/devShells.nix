{
  pkgs,
  crane,
  checks,
}:
{
  default = crane.devShell {
    inherit checks;

    packages = with pkgs; [
      rust-analyzer
    ];
  };
}
