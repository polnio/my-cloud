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
    shellHook = ''
      export DATABASE_URL=sqlite://$PWD/database.db
    '';
  };
}
