#!/usr/bin/env python3
"""Summarize GUI diagnostics JSON into a compact markdown artifact."""

from __future__ import annotations

import argparse
import json
from pathlib import Path


def main() -> int:
    parser = argparse.ArgumentParser(description="Summarize GUI diagnostics JSON")
    parser.add_argument("--report", required=True, help="Path to GUI diagnostics JSON")
    parser.add_argument(
        "--output",
        default="artifacts/gui-diagnostics-summary.md",
        help="Path to write markdown summary",
    )
    args = parser.parse_args()

    report_path = Path(args.report)
    if not report_path.exists():
        print(f"GUI diagnostics report missing: {report_path}")
        return 1

    try:
        payload = json.loads(report_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as err:
        print(f"Invalid GUI diagnostics JSON: {err}")
        return 1

    if not isinstance(payload, dict):
        print("GUI diagnostics payload must be an object")
        return 1

    issues = payload.get("issues", [])
    if not isinstance(issues, list):
        print("GUI diagnostics 'issues' must be an array")
        return 1

    lines = [
        "# GUI Diagnostics Summary",
        "",
        f"Source: `{report_path}`",
        "",
        "## Key Signals",
        "",
        f"- ok: {payload.get('ok')}",
        f"- python: {payload.get('python')}",
        f"- python_source: {payload.get('python_source')}",
        f"- has_display: {payload.get('has_display')}",
        f"- xvfb_available: {payload.get('xvfb_available')}",
        f"- missing_python_modules: {payload.get('missing_python_modules')}",
        f"- linux_libgl_present: {payload.get('linux_libgl_present')}",
        f"- linux_libxkbcommon_present: {payload.get('linux_libxkbcommon_present')}",
        "",
        "## Issues",
        "",
    ]

    if issues:
        for issue in issues:
            if not isinstance(issue, dict):
                continue
            lines.append(
                f"- {issue.get('category', 'unknown')}: {issue.get('message', '')}"
            )
    else:
        lines.append("- none")

    lines.append("")

    output_path = Path(args.output)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"Wrote GUI diagnostics markdown summary to {output_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
