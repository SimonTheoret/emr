# AGENTS.md

## Project Shape
- Single Rust binary crate (`Cargo.toml`, edition 2024); entrypoint is `src/main.rs`.
- The app is a record/replay proxy: axum listens on `0.0.0.0:3000`, forwards every route to a destination, and serializes request/response mocks as `.ron` files.
- `src/ir.rs` owns the serializable request/response IR; `src/storage.rs` watches and reloads `.ron` mocks from disk.
- `src/cli.rs` defines clap args, but `main.rs` does not parse them yet; destination is hard-coded to `https://example.com`, mock dir to `mocks`, and port to `3000`.

## Commands
- Use `cargo test` for the full test command; currently this runs 0 tests.
- Use `cargo clippy --all-targets --all-features` for linting.
- Use `cargo fmt --check` for formatting; stable rustfmt currently warns that `format_code_in_doc_comments = true` is unstable, but the command still succeeds.
- Use `cargo run` to start the proxy; it will create `mocks/` if missing and bind port `3000`.
- Nix users can enter the dev environment with `nix develop`; `.envrc` is just `use flake`.
- Build the statically linked package with `nix build`; build the image derivation with `nix build .#dockerImage`.

## Gotchas
- Mock serialization extension is controlled by `SERIALIZATION_EXTENSION` in `src/main.rs` and is currently `ron`.
- The flake package uses `pkgs.pkgsStatic.rustPlatform.buildRustPackage` and strips `$out/bin/erm` in `postInstall`.
- `ci/pipeline.sh` exists but is empty, so do not infer CI command order from it.
- No README, GitHub workflow, OpenCode config, or existing instruction file was present when this file was written.
