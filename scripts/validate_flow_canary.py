#!/usr/bin/env python3
"""Validate flow summary canary guardrails (tail, throughput, spread)."""

from __future__ import annotations

import argparse
import json
from pathlib import Path


DEFAULTS = {
    "production": {
        "tcp": {
            "min_throughput": 10000.0,
            "max_p95": 25.0,
            "max_spread": 2.5,
        },
        "inmem": {
            "min_throughput": 80000.0,
            "max_p95": 6.0,
            "max_spread": 3.5,
        },
    },
    "stress": {
        "tcp": {
            "min_throughput": 50000.0,
            "max_p95": 12.0,
            "max_spread": 2.5,
        },
        "inmem": {
            "min_throughput": 100000.0,
            "max_p95": 5.0,
            "max_spread": 3.0,
        },
    },
}


def fail(msg: str) -> int:
    print(f"Canary gate failed: {msg}")
    return 1


def main() -> int:
    parser = argparse.ArgumentParser(description="Validate flow summary canary guardrails")
    parser.add_argument("--summary", required=True)
    parser.add_argument("--profile", choices=sorted(DEFAULTS.keys()), default="production")
    args = parser.parse_args()

    summary_path = Path(args.summary)
    if not summary_path.exists():
        return fail(f"summary missing: {summary_path}")

    payload = json.loads(summary_path.read_text(encoding="utf-8"))
    modes = payload.get("modes", payload)
    if not isinstance(modes, dict):
        return fail("summary modes must be object")

    for mode in ("tcp", "inmem"):
        if mode not in modes:
            return fail(f"missing mode in summary: {mode}")
        current = modes[mode]
        thresholds = DEFAULTS[args.profile][mode]

        try:
            throughput = float(current["throughput_avg"])
            p95 = float(current["p95_avg"])
            t_min = float(current["throughput_min"])
            t_max = float(current["throughput_max"])
        except (KeyError, ValueError, TypeError) as err:
            return fail(f"invalid metric payload for {mode}: {err}")

        spread = (t_max / t_min) if t_min > 0 else float("inf")

        if throughput < thresholds["min_throughput"]:
            return fail(
                f"{mode} throughput {throughput:.2f} < {thresholds['min_throughput']:.2f}"
            )
        if p95 > thresholds["max_p95"]:
            return fail(f"{mode} p95 {p95:.2f}ms > {thresholds['max_p95']:.2f}ms")
        if spread > thresholds["max_spread"]:
            return fail(
                f"{mode} spread {spread:.2f}x > {thresholds['max_spread']:.2f}x"
            )

        print(
            f"Canary passed for {mode}: throughput={throughput:.2f}, p95={p95:.2f}ms, spread={spread:.2f}x"
        )

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
