# Repository Guidelines

## Project Structure & Module Organization

`erm` is a Rust 2024 single-binary record/replay HTTP proxy. The entry point and request flow live in `src/main.rs`. Supporting modules are split by responsibility: `src/cli.rs` defines Clap arguments, `src/ir.rs` contains serializable HTTP representations, `src/storage.rs` manages RON mock files and watching, and `src/matching.rs` selects recorded exchanges. Unit and property tests live beside the code under `#[cfg(test)]`; Proptest failure seeds are kept in `proptest-regressions/`. Architecture diagrams are `erm-diagram.drawio` and `erm-diagram.jpg`. Runtime recordings are written under `mocks/` and should not be committed.

## Build, Test, and Development Commands

- `nix develop` enters the reproducible development shell with Rust, rust-analyzer, and Cargo tools.
- `cargo run -- -d https://example.com` starts the proxy; `--mock-dir` defaults to `mocks` and `--port` to `4321`.
- `cargo build --all-targets` compiles the binary and test targets.
- `cargo test` runs all unit and Proptest tests; use `cargo test matching::` to narrow by name.
- `cargo clippy --all-targets --all-features` checks common correctness and style issues.
- `cargo fmt --check` verifies formatting. The stable toolchain may warn about an unstable rustfmt option while still succeeding.
- `nix build` creates the static release package; `nix build .#dockerImage` creates the container image.

## Coding Style & Naming Conventions

Use standard Rust formatting with four-space indentation and run `cargo fmt`. `rustfmt.toml` enables import reordering. Follow Rust naming conventions: `snake_case` for functions/modules, `CamelCase` for types and traits, and `SCREAMING_SNAKE_CASE` for constants. Keep HTTP wrapper types suffixed with `IR` (for example, `HeadersIR`) and propagate recoverable failures with `Result`/`eyre` rather than adding unchecked panics.

## Testing Guidelines

Add focused tests in the module being changed. Use Proptest for serialization round trips and matching invariants, and preserve newly generated regression files. There is no enforced coverage threshold; cover success cases, malformed data, repeated headers, streaming frames, and filesystem watcher behavior where relevant. Run `cargo test` and Clippy before submitting.

## Commit & Pull Request Guidelines

History generally uses short, imperative Conventional Commit-style subjects such as `feat:`, `fix:`, `tests:`, `build(dev):`, and `docs:`. Keep each commit scoped to one concern. Pull requests should explain the behavior change, list validation commands, link related issues, and call out mock-format or CLI compatibility changes. Include an updated diagram or terminal example when request flow or user-visible behavior changes.
