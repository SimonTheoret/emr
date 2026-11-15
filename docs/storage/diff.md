# Storage: code versus spec

Differences between the code and [the storage spec](spec.md). Delete an entry when the code matches the spec.

## 2. Architecture constraints

- **Platforms.** CI runs `cargo test` only on Linux. It builds on Windows and macOS but runs no tests there.

## 10. Quality requirements

- **Q8.** CI never builds without the `ron` feature.
