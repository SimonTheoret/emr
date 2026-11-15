# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project overview

`erm` is a record/replay HTTP proxy (single Rust binary, edition 2024). `erm record` forwards
every request to a real upstream via `axum` + `reqwest`, streams the response back, and saves the
request and its response as one exchange file named after the request's `x-erm-match` header.
Body chunks are saved with their inter-frame timing. `erm replay` answers from those files with the
recorded pacing and never contacts an upstream. `docs/storage/spec.md` is the storage spec, and
`docs/storage/diff.md` lists where the code still differs from it.

## Commands

- `cargo test` — unit and proptest tests beside the code, plus `tests/shutdown.rs` (Unix only) and
  `tests/startup.rs`, which run the built `erm` binary. `cargo test --all-features` also covers the
  JSON format; CI runs it on Linux, macOS, and Windows.
- `cargo test <name>` — run tests by substring match (e.g. `cargo test record::`).
- `cargo build --all-targets` — a plain `cargo build` does not compile test modules, so it will not
  catch a broken test call site. Also build `--no-default-features --features json`: formats are
  additive Cargo features (`ron` is the default, `json` is optional).
- `cargo clippy --all-targets --all-features` — lint; treat warnings as things to fix.
- `cargo fmt --check` — check formatting (`rustfmt.toml` sets `reorder_imports = true`; stable
  rustfmt will warn about `format_code_in_doc_comments` being unstable — that warning is expected
  and the command still succeeds).
- `cargo run -- record -d <destination-url>` and `cargo run -- replay` — start a mode. Both take
  `-m/--mock-dir` (default `mocks`; `record` creates it), `--format` (default `ron`), and `-p`
  (default `4321`).
- `nix develop` (or `direnv allow`, since `.envrc` is `use flake`) for the dev shell, which
  includes `cargo-nextest`, `cargo-insta`, `cargo-expand`, and `gdb`. It sets
  `RUSTC_WRAPPER=sccache`, so compiled crates are cached in `~/.cache/sccache` across checkouts and
  worktrees (`sccache --show-stats` shows hits).
- `nix build` builds the release binary; `nix build .#dockerImage` builds the container image.
  `tests/integration/` is a NixOS VM test (`nix build .#checks.<system>.integration`) that sends
  random requests through `erm record` to go-httpbin.
- `.github/workflows/ci.yml` runs formatting, Clippy, tests, compilation, Nix packaging, the VM
  test, and the Nix-built Docker image for pull requests and pushes to `main`.

## Architecture

- `src/main.rs` parses the CLI (`src/cli.rs`) and runs `record::run` or `replay::run`. Each mode
  serves one fallback handler for every path through `server::serve`, which shuts down gracefully
  on Ctrl-C or SIGTERM.
- Both handlers take a `MatchId` (`src/match_id.rs`), so a missing, repeated, or invalid
  `x-erm-match` gets `400` before the handler runs.
- `src/record/` forwards to `upstream_url(destination, incoming path and query)`. `capture.rs`
  wraps both bodies in a `Tap` that records each frame with its delay and hands the frames over
  only if the body ended. One task per exchange joins both taps and calls `Recorder::save`. An
  upstream error, a body error, or a client that leaves early saves nothing; a failed connection is
  `502`. Redirects are passed through, not followed. `run` waits for every `Saver` (`Drained`)
  before `Writer::finish`, so no save is lost at shutdown.
- `src/replay.rs` answers with `Replayer::find`, `404` on a miss, waiting each frame's `tslf`.
- `src/proxy.rs` builds the upstream URL, strips hop-by-hop headers, and builds the client
  response for both modes.
- `src/storage/` is the spec's storage module: `Recorder`, `Replayer`, `recorder()` (one writer
  thread, temp file then rename) and `replayer()` (loads every `<id>.<ext>` at the top level at
  startup and reports every corrupted file). Nothing outside it sees file names or formats.
- `src/ir/` is the serializable IR (`*IR` newtypes over `http`/`url` types), `TimedFrame`, and
  `FinalRequest`/`FinalResponse`. Deserializing rejects invalid header names, header values, and
  status codes, so a bad hand-edited file fails at startup.

## Gotchas

- `Cargo.toml` sets `dead_code = "deny"`. In this binary crate, an unused `pub` item fails the
  build.
- `.ron` files are gitignored, so ripgrep skips recordings under `mocks/` — read them directly.
- Inside an `async move` block, `saver.recorder.save(..)` captures only the `recorder` field, and
  the task stops holding off `Drained`. Call `Saver::save`, which borrows the whole `Saver`.
- hyper drops a body without polling it to `None` for HEAD, 204, and 304 responses and for
  exact-length bodies. `Tap`'s drop rule (`is_end_stream()` means complete) is what records them.
- `client_response` strips `content-length` and hyper regenerates it from the body's size hint, so
  a recorded HEAD response reaches the client with `content-length: 0`, and replay always answers
  with a chunked body.
- The upstream always gets a chunked request body, because `reqwest::Body::wrap` needs a `Sync`
  body and axum's is not. The `x-erm-match` header is forwarded and recorded.
- `HeadersIR` keeps repeated headers in order. It relies on the `http::HeaderMap` iterator
  invariant that a `None` name means "same name as the previous value".
- Run one ERM process per mock directory: two recorders write the same `<id>.<ext>.tmp`.
- `erm-diagram.drawio` / `.jpg` at the repo root is the architecture diagram; `.drawio.bkp` and
  `.drawio.dtmp` files are Draw.io backups and are gitignored.
