#!/usr/bin/env python3
"""Run active TCP probes and emit a machine-readable summary."""

from __future__ import annotations

import argparse
import json
import socket
import sys
import time
from dataclasses import asdict, dataclass
from pathlib import Path


@dataclass
class ProbeResult:
    target: str
    ok: bool
    latency_ms: float
    error: str


def tcp_probe(host: str, port: int, timeout_s: float) -> ProbeResult:
    start = time.perf_counter()
    try:
        with socket.create_connection((host, port), timeout=timeout_s):
            elapsed = (time.perf_counter() - start) * 1000.0
            return ProbeResult(f"{host}:{port}", True, elapsed, "")
    except OSError as exc:
        elapsed = (time.perf_counter() - start) * 1000.0
        return ProbeResult(f"{host}:{port}", False, elapsed, str(exc))


def parse_target(value: str) -> tuple[str, int]:
    host, sep, port = value.rpartition(":")
    if sep == "" or not host or not port:
        raise ValueError(f"invalid target {value!r}; expected host:port")
    return host, int(port)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Active TCP probe summary")
    parser.add_argument("--target", action="append", default=[], help="Probe target host:port")
    parser.add_argument("--timeout-ms", type=int, default=750)
    parser.add_argument("--max-failure-ratio", type=float, default=0.0)
    parser.add_argument("--output", default="")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if not args.target:
        print("No --target provided", file=sys.stderr)
        return 2

    timeout_s = max(args.timeout_ms, 1) / 1000.0
    results: list[ProbeResult] = []
    for raw_target in args.target:
        host, port = parse_target(raw_target)
        results.append(tcp_probe(host, port, timeout_s))

    failures = sum(1 for r in results if not r.ok)
    ratio = failures / len(results)
    summary = {
        "total": len(results),
        "failures": failures,
        "failure_ratio": ratio,
        "results": [asdict(r) for r in results],
    }

    payload = json.dumps(summary, indent=2)
    print(payload)

    if args.output:
        out_path = Path(args.output)
        out_path.parent.mkdir(parents=True, exist_ok=True)
        out_path.write_text(payload + "\n", encoding="utf-8")

    if ratio > args.max_failure_ratio:
        return 3
    return 0


if __name__ == "__main__":
    sys.exit(main())
