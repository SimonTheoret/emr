{
  description = "A rust development shell";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
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

          manifest = builtins.fromTOML (builtins.readFile ./Cargo.toml);
          inherit (manifest.package) name version;
          overlays = [ (import rust-overlay) ];
          pkgs = import nixpkgs { inherit system overlays; };
          rust = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
          staticRust = pkgs.pkgsStatic.buildPackages.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
          rustPlatform = pkgs.makeRustPlatform {
            cargo = rust;
            rustc = rust;
          };
          staticRustPlatform = pkgs.pkgsStatic.makeRustPlatform {
            cargo = staticRust;
            rustc = staticRust;
          };
          meta = {
            description = "Record and replay proxy";
            license = pkgs.lib.licenses.mit;
            mainProgram = name;
            maintainers = [ "simon.theoret99@gmail.com" ];
          };
          installCheckPhase = ''
            runHook preInstallCheck
            test "$("$out/bin/${name}" --version)" = "${name} ${version}"
            runHook postInstallCheck
          '';
          ermPackage = rustPlatform.buildRustPackage {
            pname = name;
            inherit version;
            src = ./.;
            cargoLock.lockFile = ./Cargo.lock;
            doInstallCheck = true;
            inherit installCheckPhase;
            postInstall = ''
              ${pkgs.binutils}/bin/strip --strip-unneeded "$out/bin/${name}"
            '';
            inherit meta;
          };
          ermPackageStatic = staticRustPlatform.buildRustPackage {
            pname = name;
            inherit version;
            src = ./.;
            cargoLock.lockFile = ./Cargo.lock;
            doInstallCheck = true;
            inherit installCheckPhase;
            postInstall = ''
              ${pkgs.binutils}/bin/strip --strip-unneeded "$out/bin/${name}"
            '';
            inherit meta;
          };
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
              cargo-expand
              cargo-nextest
              cargo-insta
              dive
              fd
              gdb
              just
              shfmt
              actionlint
              cargo-outdated
              sccache
            ]);

            RUSTC_WRAPPER = "sccache";
          };

          formatter = pkgs.nixfmt;

          checks = pkgs.lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
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
