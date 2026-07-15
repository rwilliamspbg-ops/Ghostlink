#!/usr/bin/env python3
"""Summarize a doctor JSON report into a compact markdown artifact."""

from __future__ import annotations

import argparse
import json
from collections import defaultdict
from pathlib import Path


def render_context(value: object) -> str:
    if value is None:
        return "none"
    return json.dumps(value, sort_keys=True)


def collect_check_map(checks: list[dict]) -> dict[str, dict]:
    mapping: dict[str, dict] = {}
    for check in checks:
        name = check.get("name")
        if isinstance(name, str) and name:
            mapping[name] = check
    return mapping


def main() -> int:
    parser = argparse.ArgumentParser(description="Summarize doctor JSON output")
    parser.add_argument("--report", required=True, help="Path to doctor JSON report")
    parser.add_argument(
        "--output",
        default="artifacts/doctor-summary.md",
        help="Path to write markdown summary",
    )
    args = parser.parse_args()

    report_path = Path(args.report)
    if not report_path.exists():
        print(f"Doctor report missing: {report_path}")
        return 1

    try:
        payload = json.loads(report_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as err:
        print(f"Invalid doctor report JSON: {err}")
        return 1

    summary = payload.get("summary")
    checks = payload.get("checks")
    if not isinstance(summary, dict) or not isinstance(checks, list):
        print("Doctor report must contain 'summary' object and 'checks' array")
        return 1

    normalized_checks = [check for check in checks if isinstance(check, dict)]
    check_map = collect_check_map(normalized_checks)

    lines = [
        "# Doctor Report Summary",
        "",
        f"Source: `{report_path}`",
        "",
        "## Totals",
        "",
        f"- PASS: {summary.get('pass', 0)}",
        f"- WARN: {summary.get('warn', 0)}",
        f"- FAIL: {summary.get('fail', 0)}",
        "",
    ]

    network_probe = check_map.get("network-probe")
    if isinstance(network_probe, dict):
        network_context = network_probe.get("context")
        if isinstance(network_context, dict):
            lines.extend(["## Key Signals", ""])
            reachable = network_context.get("reachable")
            latency_ms = network_context.get("latency_ms")
            resolved = network_context.get("resolved")
            target = network_context.get("target")
            lines.append(f"- network-probe target: {target}")
            lines.append(f"- network-probe resolved: {resolved}")
            lines.append(f"- network-probe reachable: {reachable}")
            if latency_ms is not None:
                lines.append(f"- network-probe latency_ms: {latency_ms}")
            lines.append("")

    non_pass = [check for check in normalized_checks if check.get("status") != "PASS"]
    if non_pass:
        lines.extend(["## Non-Pass Checks", ""])
        grouped: dict[str, list[dict]] = defaultdict(list)
        for check in non_pass:
            grouped[str(check.get("area", "unknown"))].append(check)

        for area in sorted(grouped):
            lines.append(f"### {area}")
            lines.append("")
            for check in grouped[area]:
                lines.append(
                    f"- {check.get('name', 'unknown')}: {check.get('status', 'UNKNOWN')}"
                )
                lines.append(f"  detail: {check.get('detail', '')}")
                lines.append(f"  context: {render_context(check.get('context'))}")
                fix = check.get("fix")
                if fix is not None:
                    lines.append(f"  fix: {fix}")
            lines.append("")
        lines.append("")
    else:
        lines.extend(["## Non-Pass Checks", "", "- none", ""])

    output_path = Path(args.output)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"Wrote doctor markdown summary to {output_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
