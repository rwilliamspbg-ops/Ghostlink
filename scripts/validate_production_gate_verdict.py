#!/usr/bin/env python3
"""Validate production-gate verdict JSON schema contract."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

REQUIRED_DOMAIN_KEYS = [
    "doctor",
    "doctor_probe",
    "gui_diagnostics",
    "gui_smoke",
    "xdp_preflight",
    "perf_deterministic",
    "perf_stress",
]

ALLOWED_STATUS = {"PASS", "WARN", "FAIL"}
EXPECTED_SCHEMA_VERSION = "1"


def _fail(message: str) -> int:
    print(f"Verdict schema failed: {message}")
    return 1


def _validate_status_block(block: dict, label: str) -> str | None:
    if not isinstance(block, dict):
        return f"{label} must be an object"

    for key in ("status", "detail"):
        if key not in block:
            return f"{label} missing key: {key}"

    status = block.get("status")
    detail = block.get("detail")

    if status not in ALLOWED_STATUS:
        return f"{label}.status must be one of {sorted(ALLOWED_STATUS)}, got {status!r}"
    if not isinstance(detail, str):
        return f"{label}.detail must be a string"

    return None


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Validate production gate verdict JSON schema"
    )
    parser.add_argument("--file", required=True, help="Path to verdict JSON")
    args = parser.parse_args()

    path = Path(args.file)
    if not path.exists():
        return _fail(f"file not found: {path}")

    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as err:
        return _fail(f"invalid JSON ({err})")

    if not isinstance(payload, dict):
        return _fail("payload must be an object")

    for key in ("overall", "domains"):
        if key not in payload:
            return _fail(f"missing top-level key: {key}")

    schema_version = payload.get("schema_version")
    if schema_version != EXPECTED_SCHEMA_VERSION:
        return _fail(
            "schema_version mismatch: "
            f"expected {EXPECTED_SCHEMA_VERSION!r}, got {schema_version!r}"
        )

    err = _validate_status_block(payload.get("overall"), "overall")
    if err:
        return _fail(err)

    domains = payload.get("domains")
    if not isinstance(domains, dict):
        return _fail("domains must be an object")

    missing_domains = [key for key in REQUIRED_DOMAIN_KEYS if key not in domains]
    if missing_domains:
        return _fail(f"missing domain keys: {', '.join(missing_domains)}")

    for domain in REQUIRED_DOMAIN_KEYS:
        err = _validate_status_block(domains.get(domain), f"domains.{domain}")
        if err:
            return _fail(err)

    print(f"Verdict schema passed: {path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
