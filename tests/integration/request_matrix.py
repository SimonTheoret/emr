#!/usr/bin/env python3
"""Deterministic HTTP/1.1 integration cases for ERM."""

from __future__ import annotations

import argparse
import base64
import hashlib
import http.client
import json
import random
import sys
import urllib.parse
from dataclasses import dataclass
from typing import Iterable


@dataclass(frozen=True)
class RequestCase:
    method: str
    target: str
    headers: tuple[tuple[str, str], ...]
    body: bytes

    def reproduction(self) -> str:
        return json.dumps(
            {
                "method": self.method,
                "target": self.target,
                "headers": self.headers,
                "body_base64": base64.b64encode(self.body).decode("ascii"),
            },
            sort_keys=True,
        )


@dataclass(frozen=True)
class Response:
    status: int
    headers: tuple[tuple[str, str], ...]
    body: bytes


def send_request(host: str, port: int, case: RequestCase) -> Response:
    connection = http.client.HTTPConnection(host, port, timeout=15)
    try:
        connection.putrequest(
            case.method,
            case.target,
            skip_host=True,
            skip_accept_encoding=True,
        )
        for name, value in case.headers:
            connection.putheader(name, value)
        connection.endheaders(case.body)
        response = connection.getresponse()
        return Response(response.status, tuple(response.getheaders()), response.read())
    finally:
        connection.close()


def header_multimap(headers: Iterable[tuple[str, str]]) -> dict[str, list[str]]:
    result: dict[str, list[str]] = {}
    for name, value in headers:
        result.setdefault(name.lower(), []).append(value)
    return result


def echo_header_multimap(document: dict[str, object]) -> dict[str, list[str]]:
    raw_headers = document["headers"]
    assert isinstance(raw_headers, dict)
    result: dict[str, list[str]] = {}
    for name, value in raw_headers.items():
        if isinstance(value, list):
            result[name.lower()] = [str(item) for item in value]
        else:
            result[name.lower()] = [str(value)]
    return result


def echoed_body(document: dict[str, object]) -> bytes:
    data = document["data"]
    assert isinstance(data, str)
    if data.startswith("data:") and ";base64," in data:
        encoded = data.split(",", 1)[1]
        return base64.urlsafe_b64decode(encoded + "=" * (-len(encoded) % 4))
    return data.encode("utf-8")


def verify_echo(case: RequestCase, port: int, upstream_port: int) -> None:
    response = send_request("127.0.0.1", port, case)
    assert response.status == 200, response
    document = json.loads(response.body)
    assert document["method"] == case.method

    echoed_url = urllib.parse.urlsplit(document["url"])
    incoming_url = urllib.parse.urlsplit(case.target)
    assert echoed_url.path == f"/anything{incoming_url.path}"
    assert urllib.parse.parse_qsl(echoed_url.query, keep_blank_values=True) == (
        urllib.parse.parse_qsl(incoming_url.query, keep_blank_values=True)
    )
    actual_body = echoed_body(document)
    assert actual_body == case.body, {
        "actual_length": len(actual_body),
        "expected_length": len(case.body),
        "actual_sha256": hashlib.sha256(actual_body).hexdigest(),
        "expected_sha256": hashlib.sha256(case.body).hexdigest(),
    }

    expected = header_multimap(case.headers)
    actual = echo_header_multimap(document)
    for name in [
        "x-erm-match",
        "x-case",
        "x-repeat",
        "user-agent",
        "forwarded",
        "via",
        "x-forwarded-for",
    ]:
        if name in expected:
            assert actual.get(name) == expected[name], (name, expected, actual)

    assert actual.get("host") == [f"upstream:{upstream_port}"]
    for name in [
        "connection",
        "keep-alive",
        "proxy-authorization",
        "proxy-connection",
        "te",
        "trailer",
        "upgrade",
        "x-remove-me",
    ]:
        assert name not in actual, (name, actual)


def generated_cases(seed: int, count: int) -> list[RequestCase]:
    rng = random.Random(seed)
    # go-httpbin handles OPTIONS itself instead of routing it through /anything.
    methods = ["GET", "POST", "PUT", "PATCH", "DELETE"]
    cases: list[RequestCase] = []

    for index in range(count):
        method = rng.choice(methods)
        segment = urllib.parse.quote(
            "".join(rng.choice("abcXYZ 09-._~/") for _ in range(rng.randint(0, 20))),
            safe="-._~",
        )
        query = urllib.parse.urlencode(
            [
                ("case", str(index)),
                ("repeated", "one"),
                ("repeated", "two"),
                ("empty", ""),
                ("encoded", f"value / {rng.randint(0, 999)}"),
            ]
        )
        # Keep the randomized segment from becoming the special "." or ".."
        # path segment, which URL clients are permitted to normalize.
        target = f"/case/{index}/v{segment}?{query}"

        # Payload semantics are not defined for GET or DELETE. Keep those cases
        # bodyless and exercise arbitrary binary payloads on POST/PUT/PATCH.
        size = (
            0
            if method in {"GET", "DELETE"}
            else rng.choice([0, 1, rng.randint(2, 512), rng.randint(513, 8192)])
        )
        body = bytes(rng.randrange(256) for _ in range(size))
        headers: list[tuple[str, str]] = [
            ("Host", "client.invalid"),
            ("Content-Length", str(len(body))),
            ("Content-Type", "application/octet-stream"),
            ("User-Agent", "erm-integration-runner"),
            ("X-Erm-Match", f"generated-{seed}-{index}"),
            ("X-Case", str(index)),
            ("X-Repeat", "first"),
            ("X-Repeat", "second"),
            ("Forwarded", "for=192.0.2.1"),
            ("Via", "1.1 client.test"),
            ("X-Forwarded-For", "192.0.2.1"),
        ]
        for header_index in range(rng.randint(0, 5)):
            headers.append(
                (
                    f"X-Generated-{header_index}",
                    "".join(
                        rng.choice("abcXYZ 09-._~/")
                        for _ in range(rng.randint(0, 30))
                    ).strip(),
                )
            )
        cases.append(RequestCase(method, target, tuple(headers), body))

    return cases


def focused_cases(seed: int) -> list[RequestCase]:
    return [
        RequestCase(
            "POST",
            "/focused/hop-by-hop?empty=&repeated=one&repeated=two",
            (
                ("Host", "client.invalid"),
                ("Content-Length", "8"),
                ("Content-Type", "application/octet-stream"),
                ("User-Agent", "erm-integration-runner"),
                ("X-Erm-Match", f"focused-{seed}"),
                ("X-Case", "hop-by-hop"),
                ("X-Repeat", "first"),
                ("X-Repeat", "second"),
                ("Connection", "keep-alive, x-remove-me"),
                ("Keep-Alive", "timeout=5"),
                ("X-Remove-Me", "secret"),
                ("Proxy-Authorization", "Basic dGVzdDp0ZXN0"),
                ("Proxy-Connection", "keep-alive"),
                ("TE", "trailers"),
                ("Forwarded", "for=192.0.2.1"),
                ("Via", "1.1 client.test"),
                ("X-Forwarded-For", "192.0.2.1"),
            ),
            b"hop-body",
        ),
        RequestCase(
            "PUT",
            "/focused/binary?encoded=%2F&unicode=%E2%98%83",
            (
                ("Host", "client.invalid"),
                ("Content-Length", str(65536)),
                ("Content-Type", "application/octet-stream"),
                ("User-Agent", "erm-integration-runner"),
                ("X-Erm-Match", f"binary-{seed}"),
                ("X-Case", "binary"),
            ),
            bytes(range(256)) * 256,
        ),
    ]


IGNORED_RESPONSE_HEADERS = {
    "connection",
    "content-length",
    "date",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "proxy-connection",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
    "x-remove-response",
}


def normalized_response_headers(
    headers: Iterable[tuple[str, str]],
) -> dict[str, list[str]]:
    result = header_multimap(headers)
    for name in IGNORED_RESPONSE_HEADERS:
        result.pop(name, None)
    return result


def verify_response_fidelity(response_port: int) -> None:
    targets = [
        "/status/418",
        "/bytes/4096?seed=42",
        "/drip?duration=0.05&numbytes=32&code=201&delay=0",
        (
            "/response-headers?X-Multi=one&X-Multi=two"
            "&X-End-To-End=value&Connection=x-remove-response"
            "&X-Remove-Response=secret"
        ),
    ]
    for target in targets:
        upstream_case = RequestCase(
            "GET",
            target,
            (("Host", "upstream:8080"), ("Content-Length", "0")),
            b"",
        )
        proxy_case = RequestCase(
            "GET",
            target,
            (
                ("Host", "client.invalid"),
                ("Content-Length", "0"),
                ("X-Erm-Match", f"response-{target}"),
            ),
            b"",
        )
        direct = send_request("upstream", 8080, upstream_case)
        proxied = send_request("127.0.0.1", response_port, proxy_case)
        assert proxied.status == direct.status, (target, direct, proxied)
        assert proxied.body == direct.body, (target, direct, proxied)
        assert normalized_response_headers(proxied.headers) == (
            normalized_response_headers(direct.headers)
        ), (target, direct, proxied)

        proxied_headers = header_multimap(proxied.headers)
        for name in ["connection", "x-remove-response"]:
            assert name not in proxied_headers, (target, proxied_headers)

    repeated = send_request(
        "127.0.0.1",
        response_port,
        RequestCase(
            "GET",
            "/response-headers?X-Multi=one&X-Multi=two",
            (
                ("Host", "client.invalid"),
                ("Content-Length", "0"),
                ("X-Erm-Match", "response-repeated"),
            ),
            b"",
        ),
    )
    assert header_multimap(repeated.headers).get("x-multi") == ["one", "two"]


def run(args: argparse.Namespace) -> None:
    cases = focused_cases(args.seed) + generated_cases(args.seed, args.cases)
    for label, port, upstream_port in [
        ("http", args.http_port, 8080),
        ("https", args.https_port, 8443),
    ]:
        for index, case in enumerate(cases):
            try:
                verify_echo(case, port, upstream_port)
            except Exception:
                print(
                    f"failed transport={label} seed={args.seed} index={index} "
                    f"case={case.reproduction()}",
                    file=sys.stderr,
                )
                raise

        head = send_request(
            "127.0.0.1",
            port,
            RequestCase(
                "HEAD",
                "/head",
                (
                    ("Host", "client.invalid"),
                    ("Content-Length", "0"),
                    ("X-Erm-Match", f"head-{label}"),
                ),
                b"",
            ),
        )
        assert head.status == 200
        assert head.body == b""

    verify_response_fidelity(args.response_port)
    print(
        f"passed {len(cases)} echo cases over HTTP and HTTPS "
        f"with seed {args.seed}, plus focused response cases"
    )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--seed", type=int, default=0x45524D)
    parser.add_argument("--cases", type=int, default=100)
    parser.add_argument("--http-port", type=int, default=4321)
    parser.add_argument("--https-port", type=int, default=4322)
    parser.add_argument("--response-port", type=int, default=4323)
    return parser.parse_args()


if __name__ == "__main__":
    run(parse_args())
