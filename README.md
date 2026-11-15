# erm

`erm` is a record/replay HTTP proxy for building reliable mocks around complex, slow, or unreliable upstream services.

The project aims to be simple to operate, easy to inspect, correct in how it captures HTTP traffic, and performant enough to sit in front of real development and test workloads.


## Why

Mocking HTTP services is often easy for simple JSON APIs and painful for everything else: streaming responses, flaky services, expensive calls, unusual headers, large bodies, and timing-sensitive behavior.

`erm` is intended to make those cases practical by recording real traffic once and replaying it later from editable, inspectable files.

## Run lifecycle integration tests

On x86_64 Linux, run the NixOS VM check with the packaged binary.

```sh
nix build -L .#checks.x86_64-linux.lifecycle
```

Use `lifecycle-static` to test the static binary, or replace `x86_64-linux` with `aarch64-linux` on ARM Linux.
The check runs as an unprivileged user and covers startup diagnostics, occupied ports, and SIGINT and SIGTERM shutdown in record and replay modes.
Streaming cases verify that shutdown drains an active response and persists the recording before exit.

To run the same harness locally, build `erm` and run as an unprivileged user.

```sh
cargo build
python3 tests/integration/lifecycle.py --erm "$PWD/target/debug/erm"
```
