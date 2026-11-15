# Storage: code versus spec

Differences between the code and [the storage spec](spec.md). Delete an entry when the code matches the spec.

## 8. Cross-cutting concepts

- **Redirects while recording.** ERM passes `Location` to the client unchanged, even when it points outside the destination.

## 10. Quality requirements

- **Platforms.** CI is set up to run `cargo test --all-features` on Linux, macOS, and Windows, but no macOS or Windows run has passed yet. Q6 (`tests/shutdown.rs`) runs only on Unix, because it sends SIGINT, so Ctrl-C on Windows is untested.
