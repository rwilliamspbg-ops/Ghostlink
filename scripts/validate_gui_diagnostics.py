#!/usr/bin/env python3
"""Validate machine-readable GUI diagnostics JSON."""

from __future__ import annotations

import argparse
import json
from pathlib import Path


def fail(msg: str) -> int:
    print(f"GUI diagnostics validation failed: {msg}")
    return 1


def main() -> int:
    parser = argparse.ArgumentParser(description="Validate GUI diagnostics JSON")
    parser.add_argument("--report", required=True, help="Path to GUI diagnostics JSON report")
    parser.add_argument(
        "--allow-headless",
        action="store_true",
        help="Allow headless mode when xvfb fallback is available",
    )
    parser.add_argument(
        "--require-python-source",
        action="store_true",
        help="Require the diagnostics payload to expose python_source",
    )
    args = parser.parse_args()

    report_path = Path(args.report)
    if not report_path.exists():
        return fail(f"report missing: {report_path}")

    try:
        payload = json.loads(report_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as err:
        return fail(f"invalid GUI diagnostics JSON: {err}")

    if not isinstance(payload, dict):
        return fail("GUI diagnostics payload must be an object")

    if args.require_python_source and not isinstance(payload.get("python_source"), str):
        return fail("python_source missing from GUI diagnostics payload")

    missing_python_modules = payload.get("missing_python_modules")
    if not isinstance(missing_python_modules, list):
        return fail("missing_python_modules must be an array")
    if missing_python_modules:
        return fail(f"missing_python_modules reports unresolved modules: {missing_python_modules}")

    probe_error = payload.get("python_module_probe_error")
    if probe_error is not None:
        return fail(f"python module probe error present: {probe_error}")

    for key in ("linux_libgl_present", "linux_libxkbcommon_present"):
        value = payload.get(key)
        if value is False:
            return fail(f"{key} is false")

    has_display = bool(payload.get("has_display"))
    xvfb_available = bool(payload.get("xvfb_available"))
    if not has_display and not (args.allow_headless and xvfb_available):
        return fail("headless GUI diagnostics without allowed xvfb fallback")

    print("GUI diagnostics validation passed.")
    print(f"Checked report: {report_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
