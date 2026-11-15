# erm

`erm` is a record/replay HTTP proxy for building reliable mocks around complex, slow, or unreliable upstream services.

The project aims to be simple to operate, easy to inspect, correct in how it captures HTTP traffic, and performant enough to sit in front of real development and test workloads.


## Why

Mocking HTTP services is often easy for simple JSON APIs and painful for everything else: streaming responses, flaky services, expensive calls, unusual headers, large bodies, and timing-sensitive behavior.

`erm` is intended to make those cases practical by recording real traffic once and replaying it later from editable, inspectable files.

## Usage

Record through the proxy. Every request names its exchange with an `x-erm-match` header, made of lowercase letters, digits, `-`, and `_`:

```sh
erm record --destination https://api.example.test
curl -H 'x-erm-match: login' -d '{"user":"a"}' localhost:4321/v1/login
```

ERM forwards the request to `https://api.example.test/v1/login`, streams the response back, and saves both as `mocks/login.ron`. Stop it with Ctrl-C, and it writes every exchange still in flight before it exits.

Replay without the upstream:

```sh
erm replay
curl -H 'x-erm-match: login' localhost:4321/v1/login
```

ERM answers with the recorded status, headers, and body, and waits each body chunk's recorded delay. A request without a valid `x-erm-match` gets `400`, and an unknown one gets `404`.

Both modes take `--mock-dir` (default `mocks`), `--format` (`ron` by default, or `json` when built with the `json` feature), and `-p` (default `4321`).
