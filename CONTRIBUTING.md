# Contributing to Ghost-Link

## Prerequisites

- stable Rust via `rustup`
- Git

## Setup

```bash
git clone https://github.com/rwilliamspbg-ops/Ghostlink.git
cd Ghostlink
. "$HOME/.cargo/env"
cargo build --workspace
```

## Before Opening a PR

```bash
cargo fmt
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
python3 scripts/validate_gui_api_contract.py
python3 scripts/validate_flow_canary.py --summary ./tmp/perf_snapshot/summary.json --profile production
```

## Pre-Push Checklist (Required)

Run these before pushing branch updates:

```bash
# 1) Rust correctness and style
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

# 2) Core Python contract checks
python3 scripts/validate_gui_api_contract.py
python3 scripts/validate_flow_metrics_schema_contract.py --tcp-file ./tmp/flow-metrics-tcp.json --inmem-file ./tmp/flow-metrics-inmem.json
python3 scripts/validate_production_gate_verdict.py --file ./tmp/production-gate-verdict.json

# 3) Security and dependency checks
cargo audit
python3 -m pip install --upgrade pip-audit
pip-audit -r third_party/mohawk_gui/requirements-runtime.txt

# 4) Workflow consistency sanity checks
bash scripts/check_license_consistency.sh
```

When perf/runtime code changes, also run gate-like performance checks and include the artifact paths and canary results in the PR body:

```bash
python3 scripts/flow_perf_snapshot.py --release --runs 3 --warmup-runs 1 --modes tcp inmem --output-dir ./tmp/perf_snapshot_gate
python3 scripts/flow_perf_snapshot.py --release --runs 4 --warmup-runs 1 --modes tcp inmem --exec-tokens 512 --micro-batch 8 --output-dir ./tmp/perf_snapshot_stress_gate
python3 scripts/perf_maturity_profile.py --summary ./tmp/perf_snapshot_gate/summary.json --baseline ./docs/PERF_BASELINE.json --stage-glob-template './tmp/perf_snapshot_gate/{mode}-*.json' --output-json ./tmp/perf_snapshot_gate/maturity_scorecard.json
python3 scripts/perf_maturity_profile.py --summary ./tmp/perf_snapshot_stress_gate/summary.json --baseline ./docs/PERF_BASELINE_STRESS.json --stage-glob-template './tmp/perf_snapshot_stress_gate/{mode}-*.json' --output-json ./tmp/perf_snapshot_stress_gate/maturity_scorecard.json
python3 scripts/validate_flow_canary.py --summary ./tmp/perf_snapshot_gate/summary.json --profile production
python3 scripts/validate_flow_canary.py --summary ./tmp/perf_snapshot_stress_gate/summary.json --profile stress
```

Include both `maturity_scorecard.json` artifact paths in the PR body when perf/runtime behavior changes.

If you touch integration behavior, also run:

```bash
cargo test -p ghostlink-core --test integration
```

If you touch model/bootstrap/download workflow, also run:

```bash
python3 scripts/verify_hf_models.py
```

## Test Location

Package-owned integration tests live in `crates/ghostlink-core/tests/`.

## Documentation Expectations

If behavior changes, update the relevant docs in:

- `README.md`
- `docs/INDEX.md`
- `TESTING.md`
- `docs/ARCHITECTURE.md`
- `docs/EXAMPLES.md`

If a status document is no longer current, move it to `docs/archive/` and update
`docs/archive/INDEX.md`.

## PR Expectations

- keep changes focused
- include validation commands in the PR body
- call out host-specific caveats if runtime detection or probe behavior changes

## Scope Guidance

- Prefer atomic PRs that target one theme (runtime, GUI, perf governance, or CI plumbing).
- If cross-cutting changes are unavoidable, include a short risk section and rollback strategy in the PR body.
- For large feature deliveries, consider a sequence of smaller stacked PRs.

## Release Rubric

For release-oriented PRs, include a checklist based on:

- required CI gates (production gate, tests, lint)
- runtime/perf checks (baseline drift, stage-tail/canary where applicable)
- GUI readiness checks (if GUI code changed)
- documentation completeness and operational caveats

Recommended rubric reporting format:

1. Hard gates (must pass)
2. Weighted score (for readiness trend tracking)
3. Final recommendation (GO / Conditional GO / NO-GO)
