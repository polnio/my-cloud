{
  description = "My Own Cloud Server";
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  outputs =
    { self, nixpkgs }:
    let
      systems = nixpkgs.lib.systems.flakeExposed;
      forAllSystems = nixpkgs.lib.genAttrs systems;
    in
    {
      packages = forAllSystems (import ./nix/packages.nix self);
      devShells = forAllSystems (import ./nix/shells.nix self);
      nixosModules = import ./nix/modules.nix self;
    };
}
