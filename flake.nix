{
  description = "My Rust Package";

  outputs = { self, nixpkgs }: {
    packages.${self.system} = let
      pkgs = import nixpkgs { system = self.system; };
    in
    pkgs.stdenv.mkDerivation {
      pname = "auto_redshift";
      version = "0.1.0";

      src = ./.;

      buildInputs = [ pkgs.rust ];

      buildPhase = ''
        cargo build --release
      '';

      installPhase = ''
        mkdir -p $out/bin
        cp target/release/auto_redshift $out/bin/
      '';
    };
  };
}
