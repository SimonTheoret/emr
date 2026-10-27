{
  description = "A rust development shell";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
      flake-utils,
      ...
    }:
    flake-utils.lib.eachSystem [
      "x86_64-linux"
      "aarch64-linux"
      "aarch64-darwin"
    ] (
      system:
      let

        name = "erm";
        version = "0.1.0";
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };
        rust = pkgs.rust-bin.stable.latest.default.override {
          extensions = [
            "rust-src"
            "rust-analyzer"
          ];
          targets = [ ];
        };
        ermPackage = pkgs.pkgsStatic.rustPlatform.buildRustPackage (finalAttrs: {
          pname = name;
          inherit version;
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
          postInstall = ''
            ${pkgs.binutils}/bin/strip $out/bin/erm
          '';
          meta = {
            description = "Record and replay proxy";
            license = pkgs.lib.licenses.mit;
            maintainers = [ "simon.theoret99@gmail.com" ];
          };
        });
        integrationTest = import ./tests/integration {
          inherit pkgs;
          erm = ermPackage;
        };
      in
      {
        packages.default = ermPackage;

        packages.dockerImage = pkgs.pkgsStatic.dockerTools.buildLayeredImage {
          name = "${name}";
          tag = "${version}";
          contents = [
            pkgs.dockerTools.caCertificates
          ];

          config = {
            Cmd = [
              "${self.packages.${system}.default}/bin/erm"
            ];
          };
        };

        devShells.default = pkgs.mkShell {
          buildInputs = [
            rust
          ]
          ++ (with pkgs; [
            pkg-config
            clippy
            cargo-expand
            cargo-nextest
            cargo-insta
            gdb
          ]);
        };

        checks = pkgs.lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
          integration = integrationTest;
        };

        apps = pkgs.lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
          integration = {
            type = "app";
            program = "${integrationTest.driverInteractive}/bin/nixos-test-driver";
          };
        };
      }
    );
}
