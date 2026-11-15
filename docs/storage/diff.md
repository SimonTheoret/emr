# Storage: code versus spec

Differences between the code and [the storage spec](spec.md). Delete an entry when the code matches the spec.

## 1. Introduction and goals

- **Modes.** The code has no modes. Every request is forwarded and recorded, and nothing is replayed.
- **Exchange.** The spec saves one exchange. The code writes `request_<uuid>.ron` and `response_<uuid>.ron` from two independent tasks, `serialize_request` and `serialize_response` in `src/main.rs`.
- **Id.** The spec's `id` is the `x-erm-match` value. The code identifies and pairs files by a UUIDv7 that `handle_request` generates for each request.
- **Find.** The code has no lookup in storage. `MockFileStorage::mocks` returns every mock keyed by file path, and `match_by_header` in `src/matching.rs` scans them. The request path never calls `match_by_header`.
- **Save.** The code writes files outside storage, from one tokio task per request and per response.
- **Hot reload.** The spec loads files only at startup. The code watches the mock directory with `notify` and applies events on each `MockFileStorage::mocks` call.
- **Encapsulation.** `src/main.rs` builds file names and writes files. `MockFileStorage::mocks` exposes file paths and the `Mock` enum. `match_by_header` pairs requests with responses itself and has an error case, `NoResponseForRequest`, for a missing response.
- **Errors.** `handle_request` panics when `MockFileStorage::mocks` returns an error.

## 2. Architecture constraints

- **Format.** RON is hard-coded, with no Cargo feature and no configuration option. `src/main.rs` owns the extension (`SERIALIZATION_EXTENSION`) and calls `Mock::to_pretty_ron` itself.
- **Module.** `src/storage/` defines the spec's API, but nothing outside it uses it yet. `main.rs` allows dead code and unused imports on `mod storage`, and still uses the old `MockFileStorage` from `src/legacy_storage.rs`.
- **Platforms.** CI runs `cargo test` only on Linux. It builds on Windows and macOS but runs no tests there.

## 3. Context and scope

- **Request handler.** `handle_request` calls neither `save` nor `find`. After it forwards the request, it calls `MockFileStorage::mocks` and uses only the mock count, for a debug log.
- **Startup.** `Args` in `src/cli.rs` has no mode or format option. `main` always builds `MockFileStorage` with the hard-coded RON extension.

## 5. Building block view

- **Types.** There are no `Id`, `Exchange`, or `Format` types. The code stores the `Mock` enum, which holds either a request or a response.
- **In-memory map.** `MockFileStorage` keys its map by file path and sits behind an `Arc<Mutex<_>>` in `AppState`, because the watcher mutates it while requests run.

## 6. Runtime view

- **Startup.** `MockFileStorage::new` creates a missing directory in every run. `load_all_files` walks subdirectories recursively, skips dot-prefixed paths, and loads nothing when the directory itself is dot-prefixed. It logs no warning for an empty directory.
- **Shutdown.** `main` calls `axum::serve` without graceful shutdown. Serializer tasks are detached, so exchanges still being written at exit are lost.

## 8. Cross-cutting concepts

- **File names.** The code names files `request_<uuid>.ron` and `response_<uuid>.ron`. There is no `Id` type, and `x-erm-match` values are not validated.
- **Atomic writes.** `serialize_request` and `serialize_response` call `std::fs::write` on the final path, so a crash can leave a partial file.
- **Filesystem errors.** `serialize_request` and `serialize_response` return errors from detached tasks, so write failures are silently dropped and ERM keeps proxying.
- **Duplicate ids.** Several files can carry the same `x-erm-match` value, and `match_by_header` returns whichever one `HashMap` iteration yields first. In the spec, the file name is the `id`, so two exchanges can't share one.
- **Corrupted files.** `MockFileStorage::load_all_files` stops at the first file that fails to parse, and `main` panics on it through `expect`. A corrupted file that the watcher picks up later makes `handle_request` panic.

## 10. Quality requirements

- **Q1.** Partly covered. Proptests in `src/ir/mock.rs` round-trip `Mock` through RON in memory, without a file or a restart.
- **Q2 to Q8.** No tests.
