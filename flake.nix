{
  description = "A rust development shell";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      nixpkgs,
      rust-overlay,
      flake-utils,
      ...
    }:
    flake-utils.lib.eachSystem
      [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ]
      (
        system:
        let

          name = "erm";
          version = "0.1.0";
          overlays = [ (import rust-overlay) ];
          pkgs = import nixpkgs { inherit system overlays; };
          # Must match nixpkgs' rustc (used to build the packages) and RUST_VERSION in
          # .github/workflows/ci.yml; the rust-version check enforces both.
          rustVersion = "1.99.0";
          rust = pkgs.rust-bin.stable.${rustVersion}.default.override {
            extensions = [
              "rust-src"
              "rust-analyzer"
            ];
            targets = [ ];
          };
          meta = {
            description = "Record and replay proxy";
            license = pkgs.lib.licenses.mit;
            mainProgram = name;
            maintainers = [ "simon.theoret99@gmail.com" ];
          };
          ermPackage = pkgs.rustPlatform.buildRustPackage (finalAttrs: {
            pname = name;
            inherit version;
            src = ./.;
            cargoLock.lockFile = ./Cargo.lock;
            postInstall = ''
              ${pkgs.binutils}/bin/strip --strip-unneeded $out/bin/erm
            '';
            inherit meta;
          });
          ermPackageStatic = pkgs.pkgsStatic.rustPlatform.buildRustPackage (finalAttrs: {
            pname = name;
            inherit version;
            src = ./.;
            cargoLock.lockFile = ./Cargo.lock;
            postInstall = ''
              ${pkgs.binutils}/bin/strip --strip-unneeded $out/bin/erm
            '';
            inherit meta;
          });
          integrationTest = import ./tests/integration {
            inherit pkgs;
            erm = ermPackage;
          };
          integrationTestStatic = import ./tests/integration {
            inherit pkgs;
            erm = ermPackageStatic;
          };
        in
        {
          packages.default = ermPackage;
          packages.static = ermPackageStatic;

          packages.dockerImage = pkgs.pkgsStatic.dockerTools.buildLayeredImage {
            name = "${name}";
            tag = "${version}";
            contents = [
              pkgs.dockerTools.caCertificates
              ermPackageStatic
            ];

            config = {
              Cmd = [
                (pkgs.lib.getExe ermPackageStatic)
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
              dive
              fd
              gdb
              just
              shfmt
            ]);
          };

          formatter = pkgs.nixfmt;

          checks = {
            rust-version = pkgs.runCommand "rust-version" { } ''
              if [ "${pkgs.rustc.version}" != "${rustVersion}" ]; then
                echo "nixpkgs ships rustc ${pkgs.rustc.version}, but rustVersion is ${rustVersion}; update rustVersion in flake.nix" >&2
                exit 1
              fi
              if ! grep -qF 'RUST_VERSION: "${rustVersion}"' ${./.github/workflows/ci.yml}; then
                echo "RUST_VERSION in .github/workflows/ci.yml does not match rustVersion ${rustVersion}" >&2
                exit 1
              fi
              touch $out
            '';
          }
          // pkgs.lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
            integration = integrationTest;
            integration-static = integrationTestStatic;
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
