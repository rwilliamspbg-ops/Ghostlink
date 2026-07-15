#!/usr/bin/env python3
"""Validate that GUI API calls are covered by the canonical API contract.

This is a static contract check used by CI to catch drift between
`third_party/mohawk_gui/main_window.py` and the canonical
`third_party/mohawk_gui/api_contract.json` endpoint manifest.
"""

from __future__ import annotations

import re
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
GUI_FILE = ROOT / "third_party" / "mohawk_gui" / "main_window.py"
CONTRACT_FILE = ROOT / "third_party" / "mohawk_gui" / "api_contract.json"

API_CALL_PATTERN = re.compile(r"api_call\(\s*f?\"(/api[^\"]+)\"")
PATH_PARAM_PATTERN = re.compile(r"\{([a-zA-Z_][a-zA-Z0-9_]*)\}")


def collect_gui_endpoints(path: Path) -> set[str]:
    raw = set(API_CALL_PATTERN.findall(path.read_text(encoding="utf-8")))
    return {
        PATH_PARAM_PATTERN.sub(lambda m: "{" + m.group(1) + "}", endpoint)
        for endpoint in raw
    }


def collect_contract_endpoints(path: Path) -> set[str]:
    payload = json.loads(path.read_text(encoding="utf-8"))
    endpoints = payload.get("endpoints", [])
    if not isinstance(endpoints, list):
        raise ValueError("api_contract.json must contain an 'endpoints' array")
    normalized = []
    for value in endpoints:
        if not isinstance(value, str) or not value.startswith("/api"):
            raise ValueError(f"invalid endpoint in api_contract.json: {value!r}")
        normalized.append(value)
    return set(normalized)


def main() -> int:
    gui_endpoints = collect_gui_endpoints(GUI_FILE)
    contract_endpoints = collect_contract_endpoints(CONTRACT_FILE)

    missing = sorted(gui_endpoints - contract_endpoints)

    if missing:
        print("GUI API contract check failed.")
        print("Endpoints used by main_window.py but missing from api_contract.json:")
        for endpoint in missing:
            print(f"- {endpoint}")
        return 1

    print("GUI API contract check passed.")
    print(
        f"Checked {len(gui_endpoints)} GUI endpoints against {len(contract_endpoints)} contract routes."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
