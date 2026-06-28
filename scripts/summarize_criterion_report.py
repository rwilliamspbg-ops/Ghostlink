#!/usr/bin/env python3
"""Summarize Criterion estimate files into one JSON report for trend tracking."""

from __future__ import annotations

import argparse
import json
from pathlib import Path


def collect_estimates(root: Path) -> dict[str, float]:
    summary: dict[str, float] = {}
    for estimates_path in root.rglob("estimates.json"):
        if "/new/" not in str(estimates_path).replace("\\", "/"):
            continue
        rel = estimates_path.relative_to(root)
        bench_key = str(rel.parent.parent)
        payload = json.loads(estimates_path.read_text(encoding="utf-8"))
        mean = payload.get("mean", {}).get("point_estimate")
        if mean is None:
            continue
        summary[bench_key] = float(mean)
    return summary


def main() -> int:
    parser = argparse.ArgumentParser(description="Summarize Criterion benchmark outputs")
    parser.add_argument("--criterion-root", default="target/criterion")
    parser.add_argument("--output", default="artifacts/criterion-summary.json")
    args = parser.parse_args()

    root = Path(args.criterion_root)
    if not root.exists():
        print(f"Criterion root not found: {root}")
        return 1

    summary = collect_estimates(root)
    output_path = Path(args.output)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(json.dumps(summary, indent=2, sort_keys=True), encoding="utf-8")
    print(f"Wrote summary for {len(summary)} benchmarks to {output_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
