{ pkgs ? import <nixpkgs> { } }: 
 let 

   cargoInfo =  builtins.fromTOML (builtins.readFile ./Cargo.toml);

 in pkgs.stdenv.mkDerivation {
  pname = cargoInfo.package.name;
  version = cargoInfo.package.version;

  src = ./.;

  buildInputs = with pkgs ; [ libusb1 cargo rustc pkg-config cacert ] ++ lib.optional stdenv.isDarwin [ darwin.apple_sdk.frameworks.AppKit  libiconv];
  installPhase = ./nix/annepro2-tools-install.sh;
}

