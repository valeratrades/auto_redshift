{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };
  outputs = { self, nixpkgs, fenix }:

    let
      supportedSystems = [ "x86_64-linux" ];
      forAllSystems = nixpkgs.lib.genAttrs supportedSystems;
      pkgsFor = nixpkgs.legacyPackages;
			manifest = (nixpkgs.lib.importTOML ./Cargo.toml).package;
			pname = manifest.name;
    in {
      packages = forAllSystems (system: {
        default =
          let
            pkgs = pkgsFor.${system};
            toolchain = fenix.packages.${system}.minimal.toolchain;
          in
          (pkgs.makeRustPlatform {
            cargo = toolchain;
            rustc = toolchain;
          }).buildRustPackage {
            pname = manifest.name;
            version = manifest.version;
            cargoLock.lockFile = ./Cargo.lock;
            src = pkgs.lib.cleanSource ./.;
          };
      });

      devShells = forAllSystems (system:
        let
          pkgs = pkgsFor.${system};
          toolchain = fenix.packages.${system}.default.toolchain;
        in {
          default = pkgs.mkShell {
            buildInputs = [ toolchain ];
          };
        }
      );

      #good ref: https://github.com/NixOS/nixpkgs/blob/04ef94c4c1582fd485bbfdb8c4a8ba250e359195/nixos/modules/services/audio/navidrome.nix#L89
      homeManagerModules."${pname}" = { config, lib, pkgs, ... }:
        let
          inherit (lib) mkEnableOption mkOption mkIf;
          inherit (lib.types) package str;
          cfg = config."${pname}";
        in
        {
          options."${pname}" = {
            enable = mkEnableOption "";

            package = mkOption {
              type = package;
              default = self.packages.${pkgs.stdenv.hostPlatform.system}.default;
              description = "The package to use.";
            };

						wakeTime = mkOption {
							type = str;
							default = "07:00";
							description = "The target time for waking-up (will influence at what time in the evening we start to shift towards red).";
						};
          };

          config = mkIf cfg.enable {
            systemd.user.services.${pname} = {
              Unit = {
                Description = "Auto Redshift";
                After = [ "graphical-session.target" ];
              };

              Install = {
                WantedBy = [ "default.target" ];
              };

              Service = {
                Type = "simple";
                ExecStart = ''${cfg.package}/bin/${pname} start ${cfg.wakeTime}'';
                Restart = "on-failure";
              };
            };

            home.packages = [ cfg.package ];
          };
        };
    };
}

