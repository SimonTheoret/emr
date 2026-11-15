# Repository Guidelines

## Project Structure & Module Organization

`erm` is a Rust 2024 single-binary record/replay HTTP proxy with two modes, `erm record` and `erm replay`. `src/main.rs` picks the mode. Supporting modules are split by responsibility: `src/cli.rs` defines Clap arguments, `src/record/` proxies and captures each exchange, `src/replay.rs` answers from recordings, `src/ir/` contains serializable HTTP representations, and `src/storage/` saves and loads exchange files (see `docs/storage/spec.md`). Unit and property tests live beside the code under `#[cfg(test)]`; `tests/` holds tests that run the `erm` binary and the NixOS VM test in `tests/integration/`. Proptest failure seeds are kept in `proptest-regressions/`. Architecture diagrams are `erm-diagram.drawio` and `erm-diagram.jpg`. Runtime recordings are written under `mocks/` and should not be committed.

## Build, Test, and Development Commands

- `nix develop` enters the reproducible development shell with Rust, rust-analyzer, and Cargo tools.
- `cargo run -- record -d https://example.com` records through the proxy, and `cargo run -- replay` answers from the recordings. Each request needs an `x-erm-match` header that names its exchange. `--mock-dir` defaults to `mocks`, `--format` to `ron`, and `-p` to `4321`.
- `cargo build --all-targets` compiles the binary and test targets; add `--no-default-features --features json` to check the JSON-only build.
- `cargo test --all-features` runs every test with every format; use `cargo test record::` to narrow by name.
- `cargo clippy --all-targets --all-features` checks common correctness and style issues.
- `cargo fmt --check` verifies formatting. The stable toolchain may warn about an unstable rustfmt option while still succeeding.
- `nix build` creates the release package (`.#static` for a static one); `nix build .#dockerImage` creates the container image.

## Coding Style & Naming Conventions

Use standard Rust formatting with four-space indentation and run `cargo fmt`. `rustfmt.toml` enables import reordering. Follow Rust naming conventions: `snake_case` for functions/modules, `CamelCase` for types and traits, and `SCREAMING_SNAKE_CASE` for constants. Keep HTTP wrapper types suffixed with `IR` (for example, `HeadersIR`) and propagate recoverable failures with `Result`/`eyre` rather than adding unchecked panics.

## Testing Guidelines

Add focused tests in the module being changed. Use Proptest for serialization round trips, and preserve newly generated regression files. There is no enforced coverage threshold; cover success cases, malformed data, repeated headers, and streaming frames where relevant, and name the storage spec scenario (Q1 to Q8) a test covers. Run `cargo test --all-features` and Clippy before submitting.

## Commit & Pull Request Guidelines

History generally uses short, imperative Conventional Commit-style subjects such as `feat:`, `fix:`, `tests:`, `build(dev):`, and `docs:`. Keep each commit scoped to one concern. Pull requests should explain the behavior change, list validation commands, link related issues, and call out mock-format or CLI compatibility changes. Include an updated diagram or terminal example when request flow or user-visible behavior changes.
