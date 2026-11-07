{
  description = "A rust development shell";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
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

          name = "erm";
          version = "0.1.0";
          overlays = [ (import rust-overlay) ];
          pkgs = import nixpkgs { inherit system overlays; };
          # The toolchain pinned in rust-toolchain.toml, used by the dev shell and to
          # build the default package.
          rust = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
          rustPlatform = pkgs.makeRustPlatform {
            cargo = rust;
            rustc = rust;
          };
          # nixpkgs' own static (musl) Rust platform: building the static package with
          # the toolchain above makes its build scripts crash.
          staticRustPlatform = pkgs.pkgsStatic.rustPlatform;
          meta = {
            description = "Record and replay proxy";
            license = pkgs.lib.licenses.mit;
            mainProgram = name;
            maintainers = [ "simon.theoret99@gmail.com" ];
          };
          ermPackage = rustPlatform.buildRustPackage {
            pname = name;
            inherit version;
            src = ./.;
            cargoLock.lockFile = ./Cargo.lock;
            postInstall = ''
              ${pkgs.binutils}/bin/strip --strip-unneeded $out/bin/erm
            '';
            inherit meta;
          };
          ermPackageStatic = staticRustPlatform.buildRustPackage {
            pname = name;
            inherit version;
            src = ./.;
            cargoLock.lockFile = ./Cargo.lock;
            postInstall = ''
              ${pkgs.binutils}/bin/strip --strip-unneeded $out/bin/erm
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
              # Provides cargo, rustc, clippy, rustfmt, rust-analyzer and rust-src from
              # rust-toolchain.toml; nixpkgs' clippy would be built for nixpkgs' rustc.
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

            # Cache compiled crates in ~/.cache/sccache, shared across checkouts and
            # worktrees.
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
