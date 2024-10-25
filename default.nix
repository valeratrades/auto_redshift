{ pkgs ? import <nixpkgs> { } }:
#let
#	manifest = (pkgs.lib.importTOML ./Cargo.toml).package;
#in
pkgs.rustPlatform.buildRustPackage rec {
  #pname = manifest.name;
  #version = manifest.version;
	pname = "auto_redshift";
	version = "0.1.0";

  cargoLock.lockFile = ./Cargo.lock;
  src = pkgs.lib.cleanSource ./.;
}
