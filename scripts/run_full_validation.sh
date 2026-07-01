#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
VENV_PATH="${ROOT_DIR}/.venv"
VENV_PYTHON="${VENV_PATH}/bin/python"

if [[ ! -x "$VENV_PYTHON" ]]; then
  echo "Python virtual environment not found at ${VENV_PATH}" >&2
  echo "Run scripts/setup_full_test_env.sh first." >&2
  exit 1
fi

cd "$ROOT_DIR"

echo "[validate] cargo test --workspace"
cargo test --workspace

echo "[validate] cargo clippy --workspace --all-targets -- -D warnings"
cargo clippy --workspace --all-targets -- -D warnings

echo "[validate] flow perf snapshot (3 runs)"
"$VENV_PYTHON" scripts/flow_perf_snapshot.py --runs 3 --release --output-dir ./tmp/perf_snapshot_full_validation

echo "[validate] perf drift check"
"$VENV_PYTHON" scripts/check_perf_drift.py --baseline ./docs/PERF_BASELINE.json --current ./tmp/perf_snapshot_full_validation/summary.json

echo "[validate] GUI strict readiness"
GHOSTLINK_PYTHON="$VENV_PYTHON" cargo run --release -p ghost-link -- gui-check --strict

echo "[validate] all checks passed"
