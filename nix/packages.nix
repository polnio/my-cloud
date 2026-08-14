self: system:
let
  pkgs = self.inputs.nixpkgs.legacyPackages.${system};
in
rec {
  default = my-cloud;
  my-cloud = pkgs.rustPlatform.buildRustPackage {
    pname = "my-cloud";
    version = "0.1.0";
    src = ../.;
    cargoLock.lockFile = ../Cargo.lock;
    meta = {
      mainProgram = "my-cloud";
      description = "My Own Cloud Server";
      homepage = "https://github.com/polnio/my-cloud";
      license = pkgs.lib.licenses.mit;
      maintainers = [ ];
    };
  };
}
