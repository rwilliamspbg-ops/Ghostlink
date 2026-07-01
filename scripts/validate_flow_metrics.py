#!/usr/bin/env python3
"""Validate Ghostlink flow runtime metrics JSON against SLO thresholds."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

SLO_PROFILES = {
    "smoke": {
        "tcp": {"min_throughput": 1000.0, "max_p95_ms": 250.0},
        "inmem": {"min_throughput": 1000.0, "max_p95_ms": 250.0},
    },
    "production": {
        "tcp": {"min_throughput": 10000.0, "max_p95_ms": 25.0},
        "inmem": {"min_throughput": 65000.0, "max_p95_ms": 6.0},
    },
}


def _fail(msg: str) -> int:
    print(f"SLO gate failed: {msg}")
    return 1


def main() -> int:
    parser = argparse.ArgumentParser(description="Validate flow metrics JSON")
    parser.add_argument("--file", required=True, help="Path to flow metrics JSON")
    parser.add_argument("--transport", choices=["tcp", "inmem"], required=True)
    parser.add_argument(
        "--profile",
        choices=sorted(SLO_PROFILES.keys()),
        default="smoke",
        help="SLO profile to use for default thresholds",
    )
    parser.add_argument(
        "--min-throughput",
        type=float,
        default=None,
        help="Override minimum throughput threshold",
    )
    parser.add_argument(
        "--max-p95-ms",
        type=float,
        default=None,
        help="Override maximum p95 latency threshold",
    )
    parser.add_argument("--min-stage-count", type=int, default=1)
    parser.add_argument("--min-token-count", type=int, default=1)
    args = parser.parse_args()

    profile_thresholds = SLO_PROFILES[args.profile][args.transport]
    min_throughput = (
        args.min_throughput
        if args.min_throughput is not None
        else profile_thresholds["min_throughput"]
    )
    max_p95_ms = (
        args.max_p95_ms
        if args.max_p95_ms is not None
        else profile_thresholds["max_p95_ms"]
    )

    path = Path(args.file)
    if not path.exists():
        return _fail(f"metrics file not found: {path}")

    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        return _fail(f"invalid JSON ({exc})")

    required = [
        "transport_mode",
        "token_count",
        "batch_count",
        "stage_count",
        "throughput_tokens_per_sec",
        "p95_token_latency_ms",
        "stage_stats",
    ]
    for key in required:
        if key not in payload:
            return _fail(f"missing key: {key}")

    if args.transport == "tcp":
        tcp_required = [
            "tcp_max_inflight_batches",
            "tcp_reconnect_attempts",
            "tcp_reconnect_backoff_ms",
        ]
        for key in tcp_required:
            if key not in payload:
                return _fail(f"missing tcp key: {key}")

    if payload["transport_mode"] != args.transport:
        return _fail(
            f"transport mismatch: expected {args.transport}, got {payload['transport_mode']}"
        )

    try:
        token_count = int(payload["token_count"])
        stage_count = int(payload["stage_count"])
        throughput = float(payload["throughput_tokens_per_sec"])
        p95 = float(payload["p95_token_latency_ms"])
        if args.transport == "tcp":
            int(payload["tcp_max_inflight_batches"])
            int(payload["tcp_reconnect_attempts"])
            int(payload["tcp_reconnect_backoff_ms"])
    except (TypeError, ValueError) as err:
        return _fail(f"invalid numeric metric values ({err})")

    if token_count < args.min_token_count:
        return _fail(f"token_count {token_count} < {args.min_token_count}")

    if stage_count < args.min_stage_count:
        return _fail(f"stage_count {stage_count} < {args.min_stage_count}")

    if throughput < min_throughput:
        return _fail(f"throughput {throughput:.2f} < {min_throughput:.2f}")

    if p95 > max_p95_ms:
        return _fail(f"p95 {p95:.2f}ms > {max_p95_ms:.2f}ms")

    stage_stats = payload.get("stage_stats", [])
    if not isinstance(stage_stats, list) or len(stage_stats) != stage_count:
        return _fail("stage_stats length does not match stage_count")

    print(
        "SLO gate passed:",
        f"profile={args.profile}",
        f"transport={payload['transport_mode']}",
        f"throughput={throughput:.2f}",
        f"p95={p95:.2f}ms",
        f"stages={stage_count}",
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
