#!/usr/bin/env python3
"""Summarize GUI dashboard smoke output into a compact markdown artifact."""

from __future__ import annotations

import argparse
from pathlib import Path


def find_result_line(lines: list[str], prefix: str) -> str | None:
    for line in lines:
        stripped = line.strip()
        if stripped.startswith(prefix):
            return stripped
    return None


def main() -> int:
    parser = argparse.ArgumentParser(description="Summarize GUI dashboard smoke output")
    parser.add_argument("--input", required=True, help="Path to raw smoke output")
    parser.add_argument(
        "--output",
        default="artifacts/gui-dashboard-smoke-summary.md",
        help="Path to write markdown summary",
    )
    args = parser.parse_args()

    input_path = Path(args.input)
    if not input_path.exists():
        print(f"GUI dashboard smoke output missing: {input_path}")
        return 1

    lines = input_path.read_text(encoding="utf-8").splitlines()
    imports = find_result_line(lines, "✅ Imports:") or "not found"
    ui_components = find_result_line(lines, "✅ UI Components:") or "not found"
    features = find_result_line(lines, "✅ Dashboard Features:") or "not found"

    overall = any("All tests PASSED" in line for line in lines)

    output_lines = [
        "# GUI Dashboard Smoke Summary",
        "",
        f"Source: `{input_path}`",
        "",
        "## Result",
        "",
        f"- overall: {'PASS' if overall else 'FAIL'}",
        f"- imports: {imports}",
        f"- ui_components: {ui_components}",
        f"- dashboard_features: {features}",
        "",
    ]

    output_path = Path(args.output)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text("\n".join(output_lines) + "\n", encoding="utf-8")
    print(f"Wrote GUI dashboard smoke summary to {output_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
