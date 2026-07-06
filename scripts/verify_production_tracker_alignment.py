#!/usr/bin/env python3
"""Verify production tracker items are aligned with enforced CI gates."""

from __future__ import annotations

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parent.parent


def _read(rel: str) -> str:
    return (ROOT / rel).read_text(encoding="utf-8")


def _require(text: str, needle: str, source: str, failures: list[str]) -> None:
    if needle not in text:
        failures.append(f"missing '{needle}' in {source}")


def main() -> int:
    tracker = _read("docs/PRODUCTION_PHASE_TRACKER.md")
    ci = _read(".github/workflows/ci.yml")
    security = _read(".github/workflows/security.yml")
    production_gate = _read(".github/workflows/production-gate.yml")
    tests_workflow = _read(".github/workflows/tests.yml")

    failures: list[str] = []

    # Ensure tracker still captures the expected hard requirements.
    _require(
        tracker,
        "CI secret scanning and advisory enforcement",
        "docs/PRODUCTION_PHASE_TRACKER.md",
        failures,
    )
    _require(
        tracker,
        "Headless GUI function-matrix CI lane | GUI / UX | P0 | DONE",
        "docs/PRODUCTION_PHASE_TRACKER.md",
        failures,
    )

    # Ensure CI includes security and production gate workflows.
    _require(ci, "security:", ".github/workflows/ci.yml", failures)
    _require(ci, "production-gate:", ".github/workflows/ci.yml", failures)
    _require(ci, "Clippy Check (Tauri crate)", ".github/workflows/ci.yml", failures)
    _require(ci, "--fail-under 44", ".github/workflows/ci.yml", failures)

    # Ensure security workflow still includes secret scanning and advisory checks.
    _require(security, "gitleaks/gitleaks-action", ".github/workflows/security.yml", failures)
    _require(security, "cargo audit", ".github/workflows/security.yml", failures)

    # Ensure headless dashboard smoke remains in production gate.
    _require(
        production_gate,
        "GUI Dashboard Smoke (Headless)",
        ".github/workflows/production-gate.yml",
        failures,
    )
    _require(production_gate, "xvfb-run", ".github/workflows/production-gate.yml", failures)

    # Ensure tests workflow keeps key quality and integration lanes.
    _require(tests_workflow, "rust-quality:", ".github/workflows/tests.yml", failures)
    _require(tests_workflow, "frontend-build-smoke:", ".github/workflows/tests.yml", failures)
    _require(tests_workflow, "gui-backend-smoke:", ".github/workflows/tests.yml", failures)
    _require(tests_workflow, "Clippy (Tauri crate)", ".github/workflows/tests.yml", failures)
    _require(tests_workflow, "scripts/test_api_contract.py", ".github/workflows/tests.yml", failures)

    if failures:
        print("Production tracker alignment check failed:", file=sys.stderr)
        for failure in failures:
            print(f"- {failure}", file=sys.stderr)
        return 1

    print("Production tracker alignment check passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
