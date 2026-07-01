#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
VALIDATOR="${REPO_ROOT}/scripts/validate_production_gate_verdict.py"
TESTDATA_DIR="${REPO_ROOT}/scripts/testdata/production_gate_verdict"

run_expect_pass() {
  local file="$1"
  echo "[PASS-EXPECT] ${file}"
  python3 "${VALIDATOR}" --file "${file}"
}

run_expect_fail() {
  local file="$1"
  echo "[FAIL-EXPECT] ${file}"
  if python3 "${VALIDATOR}" --file "${file}"; then
    echo "Expected validator failure but got success: ${file}" >&2
    exit 1
  fi
}

run_expect_pass "${TESTDATA_DIR}/valid_verdict.json"
run_expect_fail "${TESTDATA_DIR}/missing_domain_verdict.json"
run_expect_fail "${TESTDATA_DIR}/invalid_status_verdict.json"

echo "Production gate verdict validator smoke checks passed"
