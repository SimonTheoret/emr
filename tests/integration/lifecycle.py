"""Exercise startup diagnostics and graceful shutdown through the installed binary."""

import argparse
from concurrent.futures import ThreadPoolExecutor, TimeoutError as FutureTimeout
from contextlib import contextmanager
from dataclasses import dataclass
import http.client
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import os
from pathlib import Path
import signal
import socket
import subprocess
import tempfile
import threading
import time


TIMEOUT = 20
PREFIX = b"stream-start\n"
SUFFIX = b"stream-finished\n"
BODY = PREFIX + SUFFIX


@dataclass(frozen=True)
class StartupCase:
    name: str
    args: tuple[str, ...]
    status: int
    messages: tuple[str, ...]


@contextmanager
def process(erm, args):
    with tempfile.TemporaryFile(mode="w+") as output:
        child = subprocess.Popen(
            [erm, *map(str, args)],
            stdout=output,
            stderr=subprocess.STDOUT,
            env={**os.environ, "RUST_LOG": "warn", "NO_COLOR": "1"},
        )
        try:
            yield child, output
        finally:
            if child.poll() is None:
                child.kill()
            child.wait(timeout=TIMEOUT)
            output.seek(0)
            print(output.read(), end="", flush=True)


def free_port():
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        return listener.getsockname()[1]


def wait_for_port(child, port):
    deadline = time.monotonic() + TIMEOUT
    while time.monotonic() < deadline:
        assert child.poll() is None, "erm exited before accepting connections"
        try:
            with socket.create_connection(("127.0.0.1", port), timeout=0.2):
                return
        except OSError:
            time.sleep(0.02)
    raise AssertionError("erm did not accept connections before the deadline")


def expect_draining(child, timeout):
    try:
        child.wait(timeout=timeout)
    except subprocess.TimeoutExpired:
        return
    raise AssertionError(f"erm exited with {child.returncode} before the response drained")


def wait_for_closed_port(child, port):
    deadline = time.monotonic() + TIMEOUT
    while time.monotonic() < deadline:
        assert child.poll() is None, "erm exited while the response was still active"
        try:
            with socket.create_connection(("127.0.0.1", port), timeout=0.2):
                pass
        except ConnectionRefusedError:
            return
        time.sleep(0.02)
    raise AssertionError("erm kept accepting connections after the shutdown signal")


def open_response(port, match_id):
    connection = http.client.HTTPConnection("127.0.0.1", port, timeout=TIMEOUT)
    try:
        connection.request("GET", "/stream", headers={"x-erm-match": match_id})
        response = connection.getresponse()
        assert response.status == 200, response.status
        assert response.read(len(PREFIX)) == PREFIX, "stream prefix differs"
        return connection, response
    except BaseException:
        connection.close()
        raise


def startup_cases(erm, root):
    file_path = root / "file"
    file_path.write_text("not a directory")
    locked = root / "locked"
    locked.mkdir(mode=0o500)
    invalid = root / "invalid"
    invalid.mkdir()
    (invalid / "bad.ron").write_text("not a RON exchange")
    (invalid / "Invalid.ron").write_text("not a RON exchange")
    unreadable = invalid / "unreadable.ron"
    unreadable.write_text("not readable")
    unreadable.chmod(0)
    empty = root / "empty"
    empty.mkdir()
    denied = root / "denied"
    denied.mkdir(mode=0)
    record = ("record", "-d", "http://127.0.0.1:9")
    cases = [
        StartupCase("missing mode", (), 2, ("Usage:", "record", "replay")),
        StartupCase("missing destination", ("record",), 2, ("--destination", "required")),
        StartupCase("relative destination", ("record", "-d", "/relative"), 2, ("invalid value", "--destination")),
        StartupCase("non-HTTP destination", ("record", "-d", "ftp://example.test"), 2, ("http or https",)),
        StartupCase("destination query", ("record", "-d", "https://example.test/?query"), 2, ("must not contain a query",)),
        StartupCase("destination fragment", ("record", "-d", "https://example.test/#fragment"), 2, ("must not contain a fragment",)),
        StartupCase("invalid port", ("replay", "-p", "65536"), 2, ("invalid value", "65536")),
        StartupCase("invalid format", ("replay", "--format", "yaml"), 2, ("unknown or disabled storage format: yaml",)),
        StartupCase("record directory is file", (*record, "--mock-dir", str(file_path)), 1, ("File exists",)),
        StartupCase("record unwritable parent", (*record, "--mock-dir", str(locked / "child")), 1, ("Permission denied",)),
        StartupCase("replay directory is file", ("replay", "--mock-dir", str(file_path)), 1, (str(file_path), "cannot read the directory")),
        StartupCase("replay unreadable directory", ("replay", "--mock-dir", str(denied)), 1, (str(denied), "cannot read the directory", "Permission denied")),
        StartupCase("aggregate replay errors", ("replay", "--mock-dir", str(invalid)), 1, ("Invalid.ron", "not a valid id", "bad.ron", "not a valid ron exchange", "unreadable.ron", "cannot read the file", "Permission denied")),
    ]
    try:
        with socket.socket() as occupied:
            occupied.bind(("0.0.0.0", 0))
            occupied.listen()
            port = str(occupied.getsockname()[1])
            for mode, args in (("record", record), ("replay", ("replay",))):
                cases.append(StartupCase(
                    f"occupied port {mode}",
                    (*args, "--mock-dir", str(empty), "-p", port),
                    1,
                    ("in use (os error 98)",),
                ))
            for case in cases:
                print(f"startup: {case.name}", flush=True)
                with process(erm, case.args) as (child, output):
                    assert child.wait(timeout=TIMEOUT) == case.status, case.name
                    output.seek(0)
                    text = output.read()
                    for message in case.messages:
                        assert message in text, (case.name, message, text)
    finally:
        unreadable.chmod(0o600)
        locked.chmod(0o700)
        denied.chmod(0o700)


def empty_replay(erm, root):
    for name, message in (("missing", "does not exist"), ("empty", "holds no exchanges")):
        print(f"startup: replay {name} directory", flush=True)
        port = free_port()
        with process(erm, ("replay", "--mock-dir", root / name, "-p", port)) as (child, output):
            wait_for_port(child, port)
            connection = http.client.HTTPConnection("127.0.0.1", port, timeout=TIMEOUT)
            try:
                connection.request("GET", "/", headers={"x-erm-match": "missing"})
                response = connection.getresponse()
                assert response.status == 404
                assert b"no recorded exchange" in response.read()
            finally:
                connection.close()
            child.send_signal(signal.SIGTERM)
            assert child.wait(timeout=TIMEOUT) == 0
            output.seek(0)
            assert message in output.read()


def idle_shutdown_matrix(erm, root):
    for mode in ("record", "replay"):
        for sig in (signal.SIGINT, signal.SIGTERM):
            print(f"idle shutdown: {mode} {sig.name}", flush=True)
            directory = root / f"idle-{mode}-{sig.name.lower()}"
            directory.mkdir()
            port = free_port()
            args = [mode, "--mock-dir", directory, "-p", port]
            if mode == "record":
                args.extend(("-d", "http://127.0.0.1:9"))
            with process(erm, args) as (child, _output):
                wait_for_port(child, port)
                child.send_signal(sig)
                assert child.wait(timeout=TIMEOUT) == 0, "idle shutdown failed"


@contextmanager
def upstream():
    active = threading.Event()
    release = threading.Event()
    errors = []

    class Handler(BaseHTTPRequestHandler):
        protocol_version = "HTTP/1.1"

        def do_GET(self):
            try:
                self.connection.settimeout(TIMEOUT)
                self.send_response(200)
                self.send_header("Content-Length", str(len(BODY)))
                self.end_headers()
                self.wfile.write(PREFIX)
                self.wfile.flush()
                active.set()
                if not release.wait(TIMEOUT):
                    raise TimeoutError("stream barrier was never released")
                self.wfile.write(SUFFIX)
                self.wfile.flush()
            except BaseException as error:
                errors.append(error)

        def log_message(self, *_args):
            pass

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    server.daemon_threads = True
    worker = threading.Thread(target=server.serve_forever, daemon=True)
    worker.start()
    try:
        yield server.server_port, active, release
    finally:
        release.set()
        server.shutdown()
        server.server_close()
        worker.join(timeout=TIMEOUT)
        assert not worker.is_alive(), "upstream did not stop"
        assert not errors, errors


def shutdown_matrix(erm, root):
    for sig in (signal.SIGINT, signal.SIGTERM):
        directory = root / sig.name.lower()
        match_id = sig.name.lower()
        print(f"shutdown: record {sig.name}", flush=True)
        with upstream() as (upstream_port, active, release):
            port = free_port()
            args = ("record", "-d", f"http://127.0.0.1:{upstream_port}", "--mock-dir", directory, "-p", port)
            with process(erm, args) as (child, _output):
                wait_for_port(child, port)
                connection, response = open_response(port, match_id)
                try:
                    assert active.wait(TIMEOUT), "upstream has not started the response"
                    child.send_signal(sig)
                    wait_for_closed_port(child, port)
                    expect_draining(child, timeout=3)
                    release.set()
                    assert response.read() == SUFFIX, "record response was truncated"
                    assert child.wait(timeout=TIMEOUT) == 0
                finally:
                    release.set()
                    connection.close()
        assert sorted(path.name for path in directory.iterdir()) == [f"{match_id}.ron"], "shutdown did not persist exactly one complete exchange"
        recorded = (directory / f"{match_id}.ron").read_bytes()

        print(f"shutdown: replay {sig.name}", flush=True)
        port = free_port()
        with process(erm, ("replay", "--mock-dir", directory, "-p", port)) as (child, _output):
            wait_for_port(child, port)
            connection, response = open_response(port, match_id)
            try:
                with ThreadPoolExecutor(max_workers=1) as reader:
                    remaining = reader.submit(response.read)
                    try:
                        remaining.result(timeout=0.2)
                    except FutureTimeout:
                        pass
                    else:
                        raise AssertionError("replay response finished before the shutdown signal")
                    child.send_signal(sig)
                    wait_for_closed_port(child, port)
                    assert remaining.result(timeout=TIMEOUT) == SUFFIX, "replay response was truncated"
                    assert child.wait(timeout=TIMEOUT) == 0
            finally:
                connection.close()
        assert (directory / f"{match_id}.ron").read_bytes() == recorded, "replay modified the recording"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--erm", required=True)
    args = parser.parse_args()
    assert os.geteuid() != 0, "run as an unprivileged user to exercise filesystem permissions"
    with tempfile.TemporaryDirectory(prefix="erm-lifecycle-") as directory:
        root = Path(directory)
        startup_cases(args.erm, root)
        empty_replay(args.erm, root)
        idle_shutdown_matrix(args.erm, root)
        shutdown_matrix(args.erm, root)
    print("all lifecycle cases passed", flush=True)


if __name__ == "__main__":
    main()
