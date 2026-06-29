#!/usr/bin/env python3
"""Validate that GUI API calls are backed by mock backend routes.

This is a static contract check used by CI to catch drift between
`third_party/mohawk_gui/main_window.py` and `third_party/mohawk_gui/mock_backend.py`.
"""

from __future__ import annotations

import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
GUI_FILE = ROOT / "third_party" / "mohawk_gui" / "main_window.py"
MOCK_FILE = ROOT / "third_party" / "mohawk_gui" / "mock_backend.py"

API_CALL_PATTERN = re.compile(r"api_call\(\s*\"(/api[^\"]+)\"")
ROUTE_PATTERN = re.compile(r"@app\.(?:get|post|put|delete)\(\"(/api[^\"]+)\"\)")


def collect_gui_endpoints(path: Path) -> set[str]:
    return set(API_CALL_PATTERN.findall(path.read_text(encoding="utf-8")))


def collect_mock_endpoints(path: Path) -> set[str]:
    return set(ROUTE_PATTERN.findall(path.read_text(encoding="utf-8")))


def main() -> int:
    gui_endpoints = collect_gui_endpoints(GUI_FILE)
    mock_endpoints = collect_mock_endpoints(MOCK_FILE)

    missing = sorted(gui_endpoints - mock_endpoints)

    if missing:
        print("GUI API contract check failed.")
        print("Endpoints used by main_window.py but not implemented in mock_backend.py:")
        for endpoint in missing:
            print(f"- {endpoint}")
        return 1

    print("GUI API contract check passed.")
    print(f"Checked {len(gui_endpoints)} GUI endpoints against {len(mock_endpoints)} mock routes.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
