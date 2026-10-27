# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project overview

`erm` is a record/replay HTTP proxy (single Rust binary, edition 2024). It sits in front of a real
upstream, forwards every request via `axum` + `reqwest`, and serializes both the request and the
response to `.ron` files on disk (streamed body chunks are captured with inter-frame timing so
replay can reproduce streaming behavior, not just final payloads). Today only recording is live;
replay/matching exists as a library function but is not called from the request path.

## Commands

- `cargo test` — run all tests: unit tests in `src/matching.rs` and a proptest RON round-trip test
  in `src/ir.rs`. `tests/` exists but is empty; there is no integration suite.
- `cargo test <name>` — run a single test by substring match (e.g. `cargo test matching::`).
- `cargo build --all-targets` — a plain `cargo build` does not compile test modules, so it will not
  catch a broken test call site.
- `cargo clippy --all-targets --all-features` — lint; treat warnings as things to fix.
- `cargo fmt --check` — check formatting (`rustfmt.toml` sets `reorder_imports = true`; stable
  rustfmt will warn about `format_code_in_doc_comments` being unstable — that warning is expected
  and the command still succeeds).
- `cargo run -- -d <destination-url>` — start the proxy. `-d/--destination` is required;
  `-m/--mock-dir` defaults to `mocks` (created if missing) and `-p` (port) defaults to `4321`.
- `nix develop` (or `direnv allow`, since `.envrc` is `use flake`) for the dev shell, which
  includes `cargo-nextest`, `cargo-insta`, `cargo-expand`, and `gdb`.
- `nix build` builds the statically linked release binary; `nix build .#dockerImage` builds the
  container image.
- `.github/workflows/ci.yml` runs formatting, Clippy, tests, compilation, Nix packaging, and the
  Nix-built Docker image for pull requests and pushes to `main`.

## Architecture

Request flow (see `src/main.rs`):

1. Every route (`/` and `/{*wildcard}`) hits `handle_request`, which assigns a `Uuid::now_v7()` id.
2. The incoming request is split into `ERMRequestParts` (method/version/headers/uri — cheap to
   clone, serializable) and a streamed `BodyStream` body. As body chunks arrive, each chunk plus its
   time-since-last-frame is sent over an unbounded channel to a spawned task
   (`serialize_request`/`serialize_response`) that buffers the frames and writes
   `request_<id>.ron` / `response_<id>.ron` into the mock dir once the stream ends — concurrently
   with forwarding, not after. The two halves of an exchange are separate files linked by `id`.
3. `send_request` forwards to `config.destination` via `reqwest`, wrapping the response body in the
   same tee-to-channel pattern.
4. The proxied response streams back to the caller unbuffered (`ERMResponse` implements
   `IntoResponse` directly over the body stream).

Key modules:

- `src/ir.rs` — the serializable IR (`*IR` newtypes: `MethodIR`, `VersionIR`, `HeadersIR`, `UrlIR`,
  `UriIR`, `StatusCodeIR`) that wraps `http`/`url`/`reqwest` types so they can round-trip through
  RON, plus `TimedFrame` (a body chunk + delay-since-last-frame) and the `Mock` enum
  (`Mock::Request(FinalRequest)` / `Mock::Response(FinalResponse)`) that is what gets written to
  disk.
- `src/storage.rs` — `MockFileStorage`: loads all `.ron` mocks from the mock dir into a
  `HashMap<PathBuf, Mock>` on startup and watches the directory with `notify`. Watcher events are
  **not** applied in the background: they queue on a channel and are drained lazily each time
  `mocks()` is called. Note the TOCTOU comment at the top of the file.
- `src/matching.rs` — `match_by_header`: reads the `x-erm-match` header from an incoming request,
  finds a recorded `Mock::Request` carrying the same header value, then returns the
  `Mock::Response` with that request's id. Not yet wired into `handle_request`.
- `src/cli.rs` — `clap`-derived `Args`.

## Gotchas

- `SERIALIZATION_EXTENSION` in `src/main.rs` sets the extension the storage loads (`"ron"`), but
  the serializer functions hardcode `.ron` in their file names. `.ron` files are gitignored *and*
  listed in `.ignore`, so ripgrep-based search skips files under `mocks/` — read them directly.
- Only the destination URL is used for forwarding: the incoming request's path and query are
  dropped, so every request hits exactly `--destination`.
- A mock that fails to parse is fatal: `load_all_files` propagates the error (startup fails), and a
  bad file picked up by the watcher makes `mocks()` return `Err`, which `handle_request` `expect`s.
- `remove_mocks_at_paths` only removes paths that still exist, so deleting a mock file does not
  evict it from memory until a full rescan.
- `load_all_files` walks with `skip_dotted`, so a dot-prefixed mock dir (including a
  `tempfile::TempDir`, which is dot-prefixed by default) silently loads nothing.
- `match_by_header` iterates a `HashMap`, so if two recorded requests share an `x-erm-match` value
  the one chosen is arbitrary.
- Header handling relies on an `http::HeaderMap` iterator invariant: a `None` header name means
  "reuse the previous name" (multi-value header). `HeadersIR::from(HeaderMap)` depends on this. The
  reverse conversion currently uses `insert`, which collapses repeated headers (e.g. several
  `set-cookie`s) into the last one — it should `append`.
- `erm-diagram.drawio` / `.jpg` at the repo root is the architecture diagram; `.drawio.bkp` and
  `.drawio.dtmp` files are Draw.io backups and are gitignored.
