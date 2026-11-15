# Storage: code versus spec

Differences between the code and [the storage spec](spec.md). Delete an entry when the code matches the spec.

## 1. Introduction and goals

- **Modes.** There is no Replaying mode. `erm record` is the only subcommand.

## 2. Architecture constraints

- **Platforms.** CI runs `cargo test` only on Linux. It builds on Windows and macOS but runs no tests there.

## 8. Cross-cutting concepts

- **Redirects.** reqwest follows a 303, and a 301 or 302 after a POST, as a GET without a body, so the client gets the target's response. It passes other redirects to the client with `Location` unchanged, so an absolute `Location` sends the client straight to the upstream.

## 10. Quality requirements

- **Q1 and Q2.** Partly covered. Each format's `exchanges_round_trip` proptest round-trips an exchange in memory, and `saving_an_id_again_replaces_its_file` checks the file, but no test restarts in Replaying mode.
- **Q3 and Q4.** Partly covered. Loader unit tests check both cases, but no test starts ERM.
- **Q5.** Partly covered. `a_failed_write_exits_with_status_1` runs the writer's exit path in a test process, not in the `erm` binary.
- **Q6.** No test.
- **Q8.** CI never builds without the `ron` feature.
