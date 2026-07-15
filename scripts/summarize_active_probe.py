#!/usr/bin/env python3
"""Summarize active network probe JSON into a compact markdown artifact."""

from __future__ import annotations

import argparse
import json
from pathlib import Path


def main() -> int:
    parser = argparse.ArgumentParser(description="Summarize active network probe JSON")
    parser.add_argument("--report", required=True, help="Path to active probe JSON")
    parser.add_argument(
        "--output",
        default="artifacts/doctor-probe-summary.md",
        help="Path to write markdown summary",
    )
    args = parser.parse_args()

    report_path = Path(args.report)
    if not report_path.exists():
        print(f"Active probe report missing: {report_path}")
        return 1

    try:
        payload = json.loads(report_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as err:
        print(f"Invalid active probe JSON: {err}")
        return 1

    if not isinstance(payload, dict):
        print("Active probe payload must be an object")
        return 1

    results = payload.get("results")
    if not isinstance(results, list):
        print("Active probe payload must include a 'results' array")
        return 1

    lines = [
        "# Doctor Accessibility Probe Summary",
        "",
        f"Source: `{report_path}`",
        "",
        "## Totals",
        "",
        f"- total: {payload.get('total')}",
        f"- failures: {payload.get('failures')}",
        f"- failure_ratio: {payload.get('failure_ratio')}",
        "",
        "## Results",
        "",
    ]

    if not results:
        lines.append("- none")
    else:
        for result in results:
            if not isinstance(result, dict):
                continue
            target = result.get("target")
            ok = result.get("ok")
            latency_ms = result.get("latency_ms")
            error = result.get("error")
            lines.append(f"- target={target} ok={ok} latency_ms={latency_ms} error={error}")

    lines.append("")

    output_path = Path(args.output)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"Wrote active probe markdown summary to {output_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
