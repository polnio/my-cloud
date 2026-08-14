self: system:
let
  pkgs = self.inputs.nixpkgs.legacyPackages.${system};
in
{
  default = pkgs.mkShell {
    nativeBuildInputs = with pkgs; [
      cargo
      rustc
      clippy
    ];
  };
}
