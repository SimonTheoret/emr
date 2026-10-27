# Integration tests

The integration suite boots two NixOS VMs:

- `proxy` runs ERM against HTTP and HTTPS destinations and runs the request generator.
- `upstream` runs `go-httpbin` on ports 8080 and 8443.

The committed CA, certificate, and key are deterministic test fixtures. They have no security value
and must never be reused outside this isolated VM test.

Run the CI-sized suite with:

```console
nix build .#checks.x86_64-linux.integration -L
```

Start the interactive NixOS test driver with:

```console
nix run .#integration
```

At its Python prompt, start the machines and run a larger or reproduced sample:

```python
start_all()
proxy.wait_for_unit("erm-http.service")
proxy.wait_for_unit("erm-https.service")
proxy.succeed("erm-integration-runner --seed 4543053 --cases 10000")
```

Failures print their transport, seed, case index, and a JSON request whose body is base64 encoded.
The CI suite uses seed `4543053` (`0x45524d`) and 100 generated cases per transport, in addition to
focused boundary cases.

## Protocol boundary

The first suite exercises HTTP/1.1 application semantics: ordinary methods, encoded paths and
queries, repeated headers, and empty, text, and binary bodies. HTTPS means TLS from ERM to its
upstream; ERM's listener remains plain HTTP.

The following require separate protocol-focused suites:

- **HTTP/2:** protocol negotiation, pseudo-header translation, concurrent multiplexed streams,
  stream resets, and HTTP/2 trailers.
- **CONNECT:** an authority-form request followed by an opaque, bidirectional TCP tunnel after a
  successful response.
- **Protocol upgrades:** `101 Switching Protocols`, including WebSocket, followed by bidirectional
  upgraded I/O. Supporting this requires an explicit exception to ERM's normal hop-by-hop header
  removal.

These cases are intentionally absent rather than skipped or marked as expected failures.
