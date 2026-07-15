#!/usr/bin/env python3
"""Validate flow tuning artifact schema and values."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

REQUIRED_PROFILES = ("latency", "balanced", "throughput")


def fail(msg: str) -> int:
    print(f"Flow tuning artifact invalid: {msg}")
    return 1


def main() -> int:
    parser = argparse.ArgumentParser(description="Validate flow tuning artifact")
    parser.add_argument("--file", required=True)
    args = parser.parse_args()

    path = Path(args.file)
    if not path.exists():
        return fail(f"missing file: {path}")

    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        return fail(f"invalid JSON ({exc})")

    if not isinstance(payload, dict):
        return fail("root must be object")

    profiles = payload.get("profiles")
    if not isinstance(profiles, dict):
        return fail("missing profiles object")

    for profile_name in REQUIRED_PROFILES:
        profile = profiles.get(profile_name)
        if not isinstance(profile, dict):
            return fail(f"missing profile: {profile_name}")

        for key in ("micro_batch", "tcp_max_inflight"):
            value = profile.get(key)
            if not isinstance(value, int) or value <= 0:
                return fail(f"profile {profile_name} has invalid {key}: {value}")

    print(
        "Flow tuning artifact valid:",
        ", ".join(
            f"{name}(mb={profiles[name]['micro_batch']},inflight={profiles[name]['tcp_max_inflight']})"
            for name in REQUIRED_PROFILES
        ),
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
