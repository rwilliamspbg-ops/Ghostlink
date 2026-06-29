#!/usr/bin/env python3
"""Analyze per-stage flow metrics JSON files and report percentile distributions.

This helps identify which stage is driving tail latency by aggregating stage-level
metrics across many runs.
"""

from __future__ import annotations

import argparse
import glob
import json
import statistics
from collections import defaultdict
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


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Analyze stage metrics percentiles from flow metrics JSON files"
    )
    parser.add_argument(
        "--glob",
        required=True,
        help="Glob for flow metric files, e.g. 'tmp/perf_snapshot_stress/tcp-*.json'",
    )
    parser.add_argument(
        "--fields",
        nargs="+",
        default=[
            "avg_bridge_write_ms",
            "avg_bridge_read_ms",
            "avg_recv_wait_ms",
            "avg_send_wait_ms",
            "avg_compute_ms",
        ],
        help="Stage metric fields to analyze",
    )
    args = parser.parse_args()

    files = sorted(glob.glob(args.glob))
    if not files:
        print(f"No files matched: {args.glob}")
        return 1

    by_stage: dict[int, dict[str, list[float]]] = defaultdict(
        lambda: defaultdict(list)
    )

    for file_path in files:
        payload = json.loads(Path(file_path).read_text(encoding="utf-8"))
        for stage in payload.get("stage_stats", []):
            stage_idx = int(stage.get("stage_idx", -1))
            if stage_idx < 0:
                continue
            for field in args.fields:
                value = stage.get(field)
                if value is None:
                    continue
                by_stage[stage_idx][field].append(float(value))

    print(f"files={len(files)}")
    for stage_idx in sorted(by_stage):
        print(f"stage={stage_idx}")
        for field in args.fields:
            values = by_stage[stage_idx].get(field, [])
            if not values:
                continue
            values_sorted = sorted(values)
            avg = statistics.mean(values_sorted)
            p50 = percentile(values_sorted, 0.50)
            p95 = percentile(values_sorted, 0.95)
            p99 = percentile(values_sorted, 0.99)
            print(
                f"  {field}: avg={avg:.4f} p50={p50:.4f} p95={p95:.4f} p99={p99:.4f} min={values_sorted[0]:.4f} max={values_sorted[-1]:.4f}"
            )

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
