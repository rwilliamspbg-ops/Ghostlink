#!/usr/bin/env python3
"""Validate machine-readable doctor report context fields.

This script consumes `ghost-link doctor --json` output and validates selected
structured `context` fields without scraping human-readable detail strings.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path


def fail(msg: str) -> int:
    print(f"Doctor context validation failed: {msg}")
    return 1


def build_check_map(payload: dict) -> dict[str, dict]:
    checks = payload.get("checks")
    if not isinstance(checks, list):
        raise ValueError("doctor report must contain a 'checks' array")

    mapping: dict[str, dict] = {}
    for check in checks:
        if not isinstance(check, dict):
            raise ValueError(f"invalid check payload: {check!r}")
        name = check.get("name")
        if not isinstance(name, str) or not name:
            raise ValueError(f"invalid check name: {name!r}")
        mapping[name] = check
    return mapping


def require_context(mapping: dict[str, dict], name: str) -> dict:
    if name not in mapping:
        raise ValueError(f"missing check: {name}")
    context = mapping[name].get("context")
    if not isinstance(context, dict):
        raise ValueError(f"check {name!r} missing structured context")
    return context


def main() -> int:
    parser = argparse.ArgumentParser(description="Validate doctor JSON context fields")
    parser.add_argument("--report", required=True, help="Path to doctor JSON report")
    parser.add_argument(
        "--require-network-probe",
        action="store_true",
        help="Require network-probe context and successful reachability",
    )
    parser.add_argument(
        "--allow-missing-gui-modules",
        action="store_true",
        help="Do not fail when gui-python-modules reports missing packages",
    )
    parser.add_argument(
        "--allow-headless",
        action="store_true",
        help="Do not fail when no display session is present if xvfb fallback exists",
    )
    args = parser.parse_args()

    report_path = Path(args.report)
    if not report_path.exists():
        return fail(f"report missing: {report_path}")

    try:
        payload = json.loads(report_path.read_text(encoding="utf-8"))
        mapping = build_check_map(payload)

        local_config = require_context(mapping, "local-config")
        if not bool(local_config.get("exists")):
            return fail("local-config context reports missing config")

        for name in ("deployment-guide", "systemd-template", "docker-local-demo"):
            context = require_context(mapping, name)
            if not bool(context.get("exists")):
                return fail(f"{name} context reports missing path")

        for name in ("validation-artifacts",):
            check = mapping.get(name)
            if check is None:
                return fail(f"missing check: {name}")

        gui_modules = require_context(mapping, "gui-python-modules")
        missing = gui_modules.get("missing")
        if not isinstance(missing, list):
            return fail("gui-python-modules context missing 'missing' array")
        if missing and not args.allow_missing_gui_modules:
            return fail(f"gui-python-modules context reports missing modules: {missing}")

        display = require_context(mapping, "display-session")
        has_display = bool(display.get("has_display"))
        xvfb_available = bool(display.get("xvfb_available"))
        if not has_display and not (args.allow_headless and xvfb_available):
            return fail("display-session context reports headless mode without allowed xvfb fallback")

        if args.require_network_probe:
            network = require_context(mapping, "network-probe")
            if not bool(network.get("reachable")):
                return fail("network-probe context reports unreachable target")
            if "latency_ms" not in network:
                return fail("network-probe context missing latency_ms")

    except (OSError, json.JSONDecodeError, ValueError, TypeError) as err:
        return fail(str(err))

    print("Doctor context validation passed.")
    print(f"Checked report: {report_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
