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
    tracker = _read("docs/archive/PRODUCTION_PHASE_TRACKER.md")
    ci = _read(".github/workflows/ci.yml")
    security = _read(".github/workflows/security.yml")
    production_gate = _read(".github/workflows/production-gate.yml")
    tests_workflow = _read(".github/workflows/tests.yml")

    failures: list[str] = []

    # Ensure tracker still captures the expected hard requirements.
    _require(
        tracker,
        "CI secret scanning and advisory enforcement",
        "docs/archive/PRODUCTION_PHASE_TRACKER.md",
        failures,
    )
    _require(
        tracker,
        "Headless GUI function-matrix CI lane | GUI / UX | P0 | DONE",
        "docs/archive/PRODUCTION_PHASE_TRACKER.md",
        failures,
    )

    # Ensure CI builds/tests all three toolchains directly (rust, go, frontend).
    _require(ci, "cargo build --release", ".github/workflows/ci.yml", failures)
    _require(ci, "go build ./...", ".github/workflows/ci.yml", failures)
    _require(ci, "npm run type-check", ".github/workflows/ci.yml", failures)

    # Security and production-gate now run as independently-triggered workflows
    # (push/pull_request with their own path filters) rather than as nested
    # jobs inside ci.yml. Verify they still self-trigger so the gates can't be
    # silently disabled by editing ci.yml.
    for workflow_name, content in (
        ("security", security),
        ("production-gate", production_gate),
    ):
        source = f".github/workflows/{workflow_name}.yml"
        _require(content, "push:", source, failures)
        _require(content, "pull_request:", source, failures)

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
