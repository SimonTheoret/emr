#!/usr/bin/env python3
"""Randomized, parallel echo test for erm.

Each case sends a random request through erm to go-httpbin's ``/anything``
endpoint, which answers with the request it received as JSON. That echo must
match the request we sent, except for what a proxy is expected to change:

* hop-by-hop headers and the headers named in ``Connection`` are removed;
* ``Host`` names the upstream;
* ``Content-Length`` and ``Transfer-Encoding`` describe the erm -> upstream hop
  (erm streams request bodies), so they are not compared.

Go's HTTP server drops ``Transfer-Encoding`` (and ``Trailer`` on chunked
requests) before go-httpbin sees the headers, so a leak of either cannot be
observed here; ``src/proxy.rs`` unit-tests those two.

Every input derives from the printed seed; pass ``--seed`` to reproduce a run.
"""

from __future__ import annotations

import argparse
import asyncio
import base64
import random
import secrets
import string
import sys
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass
from urllib.parse import urlsplit

import requests
from urllib3 import HTTPHeaderDict

# Mirrors HOP_BY_HOP_HEADERS in src/proxy.rs.
HOP_BY_HOP = frozenset(
    {
        "connection",
        "keep-alive",
        "proxy-authenticate",
        "proxy-authorization",
        "proxy-connection",
        "te",
        "trailer",
        "transfer-encoding",
        "upgrade",
    }
)
# Framing of the erm -> upstream hop, which erm regenerates.
REGENERATED = frozenset({"content-length", "transfer-encoding"})

METHODS = ("GET", "POST", "PUT", "PATCH", "DELETE")
# RFC 3986 unreserved characters without ".", so paths and queries never need
# percent-encoding (or dot-segment cleanup) and compare as exact strings.
URL_CHARS = string.ascii_letters + string.digits + "-_~"
NAME_CHARS = string.ascii_lowercase + string.digits
VALUE_CHARS = string.ascii_letters + string.digits + "!#$%&'()*+,-./:;=?@[]^_`|~"
TEXT_CHARS = string.printable + "éßü€漢字🦀"
# Upper bounds a body size is drawn under; skewed small to keep recorded mocks
# (and the VM disk) small while still covering multi-chunk bodies.
BODY_SIZE_CAPS = (0, 64, 1024, 16 * 1024, 64 * 1024)
TIMEOUT_SECONDS = 60


@dataclass(frozen=True)
class Case:
    name: str
    proxy: str
    upstream: str
    method: str
    target: str
    args: dict[str, list[str]]
    headers: list[tuple[str, str]]
    nominated: frozenset[str]
    body: bytes | None
    chunks: list[bytes] | None


def random_string(rng: random.Random, chars: str, low: int, high: int) -> str:
    return "".join(rng.choices(chars, k=rng.randint(low, high)))


def random_case_of(rng: random.Random, name: str) -> str:
    return "".join(c.upper() if rng.random() < 0.5 else c for c in name)


def random_value(rng: random.Random) -> str:
    # Servers trim surrounding whitespace, so only use it inside the value.
    inner = random_string(rng, VALUE_CHARS + " ", 0, 30)
    return rng.choice(VALUE_CHARS) + inner + rng.choice(VALUE_CHARS)


def unique_header_name(rng: random.Random, taken: set[str]) -> str:
    # Never "x-forwarded-*": go-httpbin derives the echoed URL scheme from it.
    while True:
        name = "x-echo-" + random_string(rng, NAME_CHARS, 4, 12)
        if name not in taken:
            taken.add(name)
            return name


def hop_by_hop_value(rng: random.Random, name: str, taken: set[str]) -> str:
    match name:
        case "keep-alive":
            return f"timeout={rng.randint(1, 60)}, max={rng.randint(1, 1000)}"
        case "proxy-authenticate":
            return f'Basic realm="{random_string(rng, URL_CHARS, 4, 12)}"'
        case "proxy-authorization":
            return "Basic " + base64.b64encode(rng.randbytes(12)).decode()
        case "proxy-connection":
            return rng.choice(("keep-alive", "close"))
        case "te":
            return rng.choice(("trailers", "gzip, trailers", "deflate;q=0.5"))
        case "trailer":
            return unique_header_name(rng, taken)
        case "upgrade":
            return rng.choice(("websocket", "erm-test/1"))
    raise ValueError(name)


def random_body(rng: random.Random) -> tuple[bytes, str]:
    size = rng.randint(0, rng.choice(BODY_SIZE_CAPS))
    if rng.random() < 0.5:
        return rng.randbytes(size), "application/octet-stream"
    text = "".join(rng.choices(TEXT_CHARS, k=size))
    return text.encode(), rng.choice(("text/plain", "text/plain; charset=utf-8"))


def split_chunks(rng: random.Random, body: bytes) -> list[bytes]:
    chunks = []
    start = 0
    while start < len(body):
        end = start + rng.randint(1, 16 * 1024)
        chunks.append(body[start:end])
        start = end
    return chunks


def generate_case(rng: random.Random, name: str, proxy: str, upstream: str) -> Case:
    method = rng.choice(METHODS)

    segments = [random_string(rng, URL_CHARS, 1, 12) for _ in range(rng.randint(1, 4))]
    path = "/anything/" + "/".join(segments)
    keys = [random_string(rng, URL_CHARS, 1, 8) for _ in range(rng.randint(1, 4))]
    query = [(rng.choice(keys), random_string(rng, URL_CHARS, 0, 10)) for _ in range(rng.randint(0, 8))]
    args: dict[str, list[str]] = {}
    for key, value in query:
        args.setdefault(key, []).append(value)
    target = path + ("?" + "&".join(f"{k}={v}" for k, v in query) if query else "")

    taken: set[str] = set()
    headers: list[tuple[str, str]] = []

    # End-to-end headers, some repeated on several lines.
    for _ in range(rng.randint(1, 6)):
        header = unique_header_name(rng, taken)
        for _ in range(rng.randint(1, 3)):
            headers.append((random_case_of(rng, header), random_value(rng)))

    for header in rng.sample(sorted(HOP_BY_HOP - REGENERATED - {"connection"}), k=rng.randint(1, 6)):
        headers.append((random_case_of(rng, header), hop_by_hop_value(rng, header, taken)))

    # Headers nominated by Connection look exactly like end-to-end ones, so erm
    # can only drop them by parsing Connection. The tokens are spread over one
    # or more Connection lines with random case and spacing.
    nominated = [unique_header_name(rng, taken) for _ in range(rng.randint(1, 3))]
    for header in nominated:
        for _ in range(rng.randint(1, 2)):
            headers.append((random_case_of(rng, header), random_value(rng)))
    tokens = [random_case_of(rng, header) for header in nominated]
    tokens += rng.sample(("keep-alive", "close"), k=rng.randint(0, 1))
    rng.shuffle(tokens)
    cuts = sorted(rng.sample(range(1, len(tokens)), k=rng.randint(0, len(tokens) - 1)))
    for start, end in zip([0, *cuts], [*cuts, len(tokens)], strict=True):
        separator = rng.choice((",", ", ", " ,", " , "))
        headers.append((random_case_of(rng, "connection"), separator.join(tokens[start:end])))

    body = None
    chunks = None
    if method != "GET":
        body, content_type = random_body(rng)
        headers.append((random_case_of(rng, "content-type"), content_type))
        if rng.random() < 0.5:
            chunks = split_chunks(rng, body)

    rng.shuffle(headers)
    return Case(
        name=name,
        proxy=proxy,
        upstream=upstream,
        method=method,
        target=target,
        args=args,
        headers=headers,
        nominated=frozenset(nominated),
        body=body,
        chunks=chunks,
    )


def send(case: Case) -> tuple[requests.Response, HTTPHeaderDict]:
    # A generator body makes requests send Transfer-Encoding: chunked.
    data = (chunk for chunk in case.chunks) if case.chunks is not None else case.body
    with requests.Session() as session:
        # Connect directly even if the environment configures an HTTP proxy.
        session.trust_env = False
        # The case brings its own Connection lines.
        del session.headers["Connection"]
        prepared = session.prepare_request(requests.Request(case.method, case.proxy + case.target, data=data))
        # requests keeps headers in a CaseInsensitiveDict, which cannot hold a
        # name twice. urllib3 writes one line per pair of an HTTPHeaderDict, so
        # swap one in after preparing to send repeated header lines.
        headers = HTTPHeaderDict(prepared.headers)
        for name, value in case.headers:
            headers.add(name, value)
        prepared.headers = headers
        response = session.send(prepared, allow_redirects=False, timeout=TIMEOUT_SECONDS)
    return response, headers


def short(value: object, limit: int = 160) -> str:
    text = repr(value)
    return text if len(text) <= limit else text[: limit - 3] + "..."


def describe_header_diff(expected: dict[str, list[str]], actual: dict[str, list[str]]) -> str:
    lines = []
    for name in sorted(expected.keys() - actual.keys()):
        lines.append(f"missing {name}: {short(expected[name])}")
    for name in sorted(actual.keys() - expected.keys()):
        lines.append(f"unexpected {name}: {short(actual[name])}")
    for name in sorted(expected.keys() & actual.keys()):
        if expected[name] != actual[name]:
            lines.append(f"{name}: sent {short(expected[name])}, upstream saw {short(actual[name])}")
    return "headers differ:\n      " + "\n      ".join(lines)


def describe_body_diff(sent: bytes, received: bytes) -> str:
    offset = next(
        (i for i, (a, b) in enumerate(zip(sent, received, strict=False)) if a != b),
        min(len(sent), len(received)),
    )
    return (
        f"body differs: sent {len(sent)} bytes, upstream saw {len(received)} bytes, "
        f"first difference at byte {offset}"
    )


def decode_data(data: str) -> bytes:
    # go-httpbin echoes text/plain bodies verbatim and everything else as a
    # data URL with URL-safe base64.
    if data.startswith("data:"):
        return base64.urlsafe_b64decode(data.partition(";base64,")[2])
    return data.encode()


def check_case(case: Case) -> list[str]:
    try:
        response, sent_headers = send(case)
    except requests.RequestException as exc:
        return [f"request failed: {exc}"]
    if response.status_code != 200:
        return [f"status {response.status_code}: {short(response.text)}"]
    try:
        document = response.json()
    except requests.JSONDecodeError:
        return [f"response is not JSON: {short(response.text)}"]

    problems = []

    if document["method"] != case.method:
        problems.append(f"method: sent {case.method}, upstream saw {document['method']}")
    expected_url = case.upstream.rstrip("/") + case.target
    if document["url"] != expected_url:
        problems.append(f"url: expected {expected_url}, upstream saw {document['url']}")
    if document["args"] != case.args:
        problems.append(f"args: sent {short(case.args)}, upstream saw {short(document['args'])}")

    removed = HOP_BY_HOP | case.nominated | {"host", "content-length"}
    expected: dict[str, list[str]] = {}
    for name, value in sent_headers.items():
        if name.lower() not in removed:
            expected.setdefault(name.lower(), []).append(value)
    expected["host"] = [urlsplit(case.upstream).netloc]

    echoed = {name.lower(): values for name, values in document["headers"].items()}
    for name in REGENERATED:
        echoed.pop(name, None)
    leaked = sorted((HOP_BY_HOP | case.nominated) & echoed.keys())
    if leaked:
        problems.append(f"hop-by-hop headers reached upstream: {', '.join(leaked)}")
    if echoed != expected:
        problems.append(describe_header_diff(expected, echoed))

    received = decode_data(document.get("data", ""))
    if received != (case.body or b""):
        problems.append(describe_body_diff(case.body or b"", received))

    return problems


async def run_all(cases: list[Case], concurrency: int) -> list[list[str] | BaseException]:
    loop = asyncio.get_running_loop()
    with ThreadPoolExecutor(max_workers=concurrency) as pool:
        tasks = [loop.run_in_executor(pool, check_case, case) for case in cases]
        return await asyncio.gather(*tasks, return_exceptions=True)


def log(message: str) -> None:
    # The NixOS test driver keeps a command's stdout but logs its stderr.
    print(message, file=sys.stderr, flush=True)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument(
        "--pair",
        nargs=2,
        action="append",
        required=True,
        metavar=("PROXY", "UPSTREAM"),
        help="erm base URL and the destination it forwards to (repeatable)",
    )
    parser.add_argument("--count", type=int, default=100, help="cases per pair (default: %(default)s)")
    parser.add_argument("--concurrency", type=int, default=16, help="requests in flight (default: %(default)s)")
    parser.add_argument("--seed", type=int, help="seed to reproduce a run (default: random)")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    seed = args.seed if args.seed is not None else secrets.randbits(32)
    log(f"seed={seed}")

    cases = [
        generate_case(random.Random(f"{seed}-{p}-{i}"), f"{p}-{i}", proxy.rstrip("/"), upstream)
        for p, (proxy, upstream) in enumerate(args.pair)
        for i in range(args.count)
    ]
    results = asyncio.run(run_all(cases, args.concurrency))

    failures = 0
    for case, result in zip(cases, results, strict=True):
        if isinstance(result, BaseException):
            result = [f"{type(result).__name__}: {result}"]
        if result:
            failures += 1
            log(f"FAIL [{case.name}] {case.method} {case.proxy}{case.target} -> {case.upstream}")
            for problem in result:
                log(f"    {problem}")

    log(f"{len(cases) - failures}/{len(cases)} cases passed (seed={seed})")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
