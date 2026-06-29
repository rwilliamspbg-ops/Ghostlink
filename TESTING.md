# Testing Guide for Ghost-Link

This document describes the current test layout, validated commands, and known gaps.

## Test Layout

```text
crates/ghostlink-core/src/
├── accelerator.rs
├── cluster.rs
├── dashboard.rs
├── health.rs
├── host.rs
├── load_balance.rs
├── planning.rs
├── protocol.rs
├── ring.rs
└── xdp.rs

crates/ghostlink-core/tests/
├── common.rs
└── integration.rs
```

The workspace root is a virtual Cargo workspace, so integration tests live under `crates/ghostlink-core/tests/` rather than a root `tests/` target.

## Validated Commands

```bash
# Build full local test environment (Python deps + GUI readiness checks)
scripts/setup_full_test_env.sh

# Run complete validation bundle (tests, clippy, perf drift, strict GUI readiness)
scripts/run_full_validation.sh

# Full workspace validation
cargo test --workspace

# Production gate checks (format + clippy + tests + runtime smoke)
GHOSTLINK_TCP_AUTH_TOKEN=local-gate cargo run -p ghost-link -- flow iprada-16gb zenbook-32gb 32 32 128 4 tcp
cargo run -p ghost-link -- flow iprada-16gb zenbook-32gb 32 32 128 4 inmem

# Export runtime metrics in JSON for SLO parsing/automation
# JSON includes stage bridge transport timing fields (avg_bridge_write_ms / avg_bridge_read_ms)
GHOSTLINK_FLOW_METRICS_JSON=./tmp/flow-metrics.json cargo run -p ghost-link -- flow iprada-16gb zenbook-32gb 32 32 128 4 tcp
python3 scripts/validate_flow_metrics.py --file ./tmp/flow-metrics.json --transport tcp --profile production

# Generate repeatable multi-run snapshot summaries
# Warmup runs are excluded from summary but help reduce cold-start variance
python3 scripts/flow_perf_snapshot.py --runs 5 --warmup-runs 1 --output-dir ./tmp/perf_snapshot
python3 scripts/check_perf_drift.py --baseline ./docs/PERF_BASELINE.json --current ./tmp/perf_snapshot/summary.json

# Stress profile validation (512 tokens / micro-batch 8)
python3 scripts/flow_perf_snapshot.py --runs 12 --warmup-runs 2 --modes tcp inmem --exec-tokens 512 --micro-batch 8 --output-dir ./tmp/perf_snapshot_stress
python3 scripts/check_perf_drift.py --baseline ./docs/PERF_BASELINE_STRESS.json --current ./tmp/perf_snapshot_stress/summary.json

# Stage-level percentile analysis for bridge/read/write waits
python3 scripts/analyze_flow_stage_metrics.py --glob './tmp/perf_snapshot_stress/tcp-*.json'
# Optional overrides for temporary policy experiments
python3 scripts/check_perf_drift.py --baseline ./docs/PERF_BASELINE.json --current ./tmp/perf_snapshot/summary.json --max-throughput-drop-ratio 0.30 --max-p95-rise-ratio 0.60

# Optional TCP inflight autotuning sweep for the current host profile
GHOSTLINK_TCP_AUTOTUNE=1 GHOSTLINK_TCP_AUTOTUNE_RUNS=3 GHOSTLINK_TCP_AUTOTUNE_CANDIDATES=32,64,128,256 \
  cargo run -p ghost-link -- flow iprada-16gb zenbook-32gb 32 32 128 4 tcp

# Optional persistent autotune cache for repeated runs on same plan/profile
GHOSTLINK_TCP_AUTOTUNE=1 GHOSTLINK_TCP_AUTOTUNE_CACHE=./tmp/tcp_autotune_cache.tsv \
  cargo run -p ghost-link -- flow iprada-16gb zenbook-32gb 32 32 128 4 tcp

# Refresh cached autotune selection when retuning after environment changes
GHOSTLINK_TCP_AUTOTUNE=1 GHOSTLINK_TCP_AUTOTUNE_REFRESH=1 \
  cargo run -p ghost-link -- flow iprada-16gb zenbook-32gb 32 32 128 4 tcp

# Enforce stage-tail SLOs across snapshot files
python3 scripts/validate_stage_tail_metrics.py --glob './tmp/perf_snapshot/tcp-*.json'

# Enforce rollout canary guardrails from summary metrics
python3 scripts/validate_flow_canary.py --summary ./tmp/perf_snapshot/summary.json --profile production

# GUI readiness and detailed diagnostics
cargo run -p ghost-link -- gui-check --strict
GHOSTLINK_GUI_DIAG_JSON=./tmp/gui-diag.json cargo run -p ghost-link -- gui-diagnose --strict

# Package-owned integration suite
cargo test -p ghostlink-core --test integration

# Lint all targets, including benches
cargo clippy --workspace --all-targets -- -D warnings

# Parse-check all Rust source files (including non-exported modules)
find crates -name '*.rs' -print0 | xargs -0 -n1 rustfmt --check --edition 2021

# Validate GUI endpoint contract drift against mock backend
python3 scripts/validate_gui_api_contract.py

# Verify Hugging Face model listing/download path
python3 scripts/verify_hf_models.py

# Run criterion benchmarks
cargo bench -p ghostlink-core --bench criterion

# Export criterion benchmark means for trend artifact publication
python3 scripts/summarize_criterion_report.py --criterion-root target/criterion --output artifacts/criterion-summary.json
```

## Current Counts

Test totals can change as modules evolve. For current totals in your environment, run:

```bash
cargo test --workspace -- --list | wc -l
```

Use CI workflow summaries as the source of truth for branch-level totals.

## Coverage Status

Coverage is reported by CI via `cargo tarpaulin` (`Lcov` output) and uploaded to Codecov, with raw coverage artifacts retained in workflow artifacts.

## Runtime Detection Tests

The host/runtime stack now includes test coverage for:

- fast cached probe mode
- full probe mode parsing helpers
- compute capability inference from detected device names
- execution backend selection and scaled slice execution

## Benchmark Coverage

Current benchmark targets cover:

- ring buffer hot paths
- protocol encode/decode
- planning and autotuned planning
- runtime detection in fast and full modes
- autotuned load balancing
- runtime-selected backend scaling

## Known Gaps

- AF_XDP/eBPF is still validated through unit-level behavior only
- Full hardware probing only exercises `nvidia-smi` and `lspci` when those tools exist on the host
- Health monitoring uses collected cluster metrics and heartbeat state instead of direct ping-style network probes
- Tensor migration and dynamic cluster rebalance remain incomplete
- GUI function-matrix validation currently runs as a local/devcontainer procedure, not a dedicated CI workflow job
