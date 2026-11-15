# Storage: code versus spec

Differences between the code and [the storage spec](spec.md). Delete an entry when the code matches the spec.

## 2. Architecture constraints

- **Platforms.** CI runs `cargo test` only on Linux. It builds on Windows and macOS but runs no tests there.

## 8. Cross-cutting concepts

- **Redirects.** reqwest follows a 303, and a 301 or 302 after a POST, as a GET without a body, so the client gets the target's response. It passes other redirects to the client with `Location` unchanged, so an absolute `Location` sends the client straight to the upstream.

## 10. Quality requirements

- **Q8.** CI never builds without the `ron` feature.
