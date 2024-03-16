{
  description = "A set of tools to flash an annepro2 keyboard with custom firmware.";
  inputs.flake-utils.url = "github:numtide/flake-utils";
  outputs = { self, nixpkgs, flake-utils } :
  flake-utils.lib.eachDefaultSystem  (
    system:
    let
      pkgs = nixpkgs.legacyPackages.${system};
      annepro2tools = pkgs.callPackage ./default.nix { };
    in
    {

      packages.annepro2-tools = annepro2tools;

      defaultPackage = annepro2tools;

      devShells.default = pkgs.mkShell {
        buildInputs = [ annepro2tools ];
      };
    }
    );
  }

