#!/usr/bin/env python3
"""Legacy GUI compatibility entrypoint.

This shim keeps strict readiness checks and older launch paths working by
forwarding execution to the maintained Mohawk GUI entrypoint.
"""

from __future__ import annotations

import runpy
import sys
from pathlib import Path


def main() -> int:
    repo_root = Path(__file__).resolve().parent
    target = repo_root / "third_party" / "mohawk_gui" / "main.py"

    if not target.exists():
        print(f"Missing GUI target: {target}", file=sys.stderr)
        return 1

    # Ensure local imports inside the GUI package resolve when launched via shim.
    sys.path.insert(0, str(target.parent))
    runpy.run_path(str(target), run_name="__main__")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
