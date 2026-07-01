#!/usr/bin/env python3
"""Validate tracked GUI Python config defaults.

This prevents sample or local tracked config files from reintroducing a generic
`python3`/`python` GUI override that defeats the repo virtualenv fallback.
"""

from __future__ import annotations

import sys
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:  # pragma: no cover
    print("Python 3.11+ is required for tomllib", file=sys.stderr)
    raise SystemExit(2)


ROOT = Path(__file__).resolve().parents[1]
CONFIG_FILES = [ROOT / "ghostlink.toml", ROOT / "ghostlink.example.toml"]
DISALLOWED = {"python", "python3"}


def main() -> int:
    failures: list[str] = []

    for path in CONFIG_FILES:
        if not path.exists():
            failures.append(f"missing config file: {path}")
            continue

        payload = tomllib.loads(path.read_text(encoding="utf-8"))
        gui = payload.get("gui")
        if not isinstance(gui, dict):
            continue

        python_value = gui.get("python")
        if not isinstance(python_value, str):
            continue

        normalized = python_value.strip()
        if normalized in DISALLOWED:
            failures.append(
                f"{path.name} sets gui.python={normalized!r}; leave it unset to prefer the repo .venv or set an explicit custom interpreter path"
            )

    if failures:
        print("GUI Python config validation failed.")
        for failure in failures:
            print(f"- {failure}")
        return 1

    print("GUI Python config validation passed.")
    print("Tracked config files do not override the repo virtualenv with a generic Python default.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())