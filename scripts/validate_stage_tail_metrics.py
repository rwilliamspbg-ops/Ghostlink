#!/usr/bin/env python3
"""Validate per-stage tail latency metrics from flow JSON files."""

from __future__ import annotations

import argparse
import glob
import json
from pathlib import Path


def percentile(sorted_values: list[float], p: float) -> float:
    if not sorted_values:
        return 0.0
    if len(sorted_values) == 1:
        return sorted_values[0]
    rank = (len(sorted_values) - 1) * p
    lo = int(rank)
    hi = min(lo + 1, len(sorted_values) - 1)
    frac = rank - lo
    return sorted_values[lo] * (1.0 - frac) + sorted_values[hi] * frac


def fail(msg: str) -> int:
    print(f"Stage-tail gate failed: {msg}")
    return 1


def main() -> int:
    parser = argparse.ArgumentParser(description="Validate stage tail latency SLOs")
    parser.add_argument("--glob", required=True, help="Glob for flow JSON files")
    parser.add_argument("--pctl", type=float, default=0.95, help="Percentile to enforce")
    parser.add_argument("--max-bridge-read-ms", type=float, default=0.25)
    parser.add_argument("--max-bridge-write-ms", type=float, default=0.08)
    parser.add_argument("--max-recv-wait-ms", type=float, default=0.25)
    parser.add_argument("--max-send-wait-ms", type=float, default=0.10)
    args = parser.parse_args()

    files = sorted(glob.glob(args.glob))
    if not files:
        return fail(f"no files matched: {args.glob}")

    by_stage: dict[int, dict[str, list[float]]] = {}
    fields = [
        "avg_bridge_read_ms",
        "avg_bridge_write_ms",
        "avg_recv_wait_ms",
        "avg_send_wait_ms",
    ]

    for file_path in files:
        payload = json.loads(Path(file_path).read_text(encoding="utf-8"))
        for stage in payload.get("stage_stats", []):
            idx = int(stage.get("stage_idx", -1))
            if idx < 0:
                continue
            by_stage.setdefault(idx, {name: [] for name in fields})
            for name in fields:
                if name in stage:
                    by_stage[idx][name].append(float(stage[name]))

    limits = {
        "avg_bridge_read_ms": args.max_bridge_read_ms,
        "avg_bridge_write_ms": args.max_bridge_write_ms,
        "avg_recv_wait_ms": args.max_recv_wait_ms,
        "avg_send_wait_ms": args.max_send_wait_ms,
    }

    for stage_idx in sorted(by_stage):
        for field, values in by_stage[stage_idx].items():
            if not values:
                continue
            pval = percentile(sorted(values), args.pctl)
            if pval > limits[field]:
                return fail(
                    f"stage {stage_idx} {field} p{int(args.pctl * 100)}={pval:.4f}ms > {limits[field]:.4f}ms"
                )

    print(
        "Stage-tail gate passed:",
        f"files={len(files)}",
        f"pctl={args.pctl}",
        f"limits={limits}",
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
