{ pkgs ? import <nixpkgs> { }
}: pkgs.mkShell {
  buildInputs = with pkgs ; [ libusb1 cargo rustc pkg-config cacert ] ++ lib.optional stdenv.isDarwin [ darwin.apple_sdk.frameworks.AppKit libiconv ];
}
