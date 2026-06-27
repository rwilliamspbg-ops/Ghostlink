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
# Full workspace validation
cargo test --workspace

# Production gate checks (format + clippy + tests + runtime smoke)
GHOSTLINK_TCP_AUTH_TOKEN=local-gate cargo run -p ghost-link -- flow iprada-16gb zenbook-32gb 32 32 128 4 tcp
cargo run -p ghost-link -- flow iprada-16gb zenbook-32gb 32 32 128 4 inmem

# Export runtime metrics in JSON for SLO parsing/automation
GHOSTLINK_FLOW_METRICS_JSON=./tmp/flow-metrics.json cargo run -p ghost-link -- flow iprada-16gb zenbook-32gb 32 32 128 4 tcp
python3 scripts/validate_flow_metrics.py --file ./tmp/flow-metrics.json --transport tcp --profile production

# Generate repeatable multi-run snapshot summaries
python3 scripts/flow_perf_snapshot.py --runs 5 --output-dir ./tmp/perf_snapshot
python3 scripts/check_perf_drift.py --baseline ./docs/PERF_BASELINE.json --current ./tmp/perf_snapshot/summary.json
# Optional overrides for temporary policy experiments
python3 scripts/check_perf_drift.py --baseline ./docs/PERF_BASELINE.json --current ./tmp/perf_snapshot/summary.json --max-throughput-drop-ratio 0.30 --max-p95-rise-ratio 0.60

# Package-owned integration suite
cargo test -p ghostlink-core --test integration

# Lint all targets, including benches
cargo clippy --workspace --all-targets -- -D warnings

# Verify Hugging Face model listing/download path
python3 scripts/verify_hf_models.py

# Run criterion benchmarks
cargo bench -p ghostlink-core --bench criterion
```

## Current Counts

The current validated workspace contains 112 passing tests:

- 3 CLI tests in `crates/ghost-link/src/main.rs`
- 75 library tests in `ghostlink-core`
- 7 tests in `crates/ghostlink-core/tests/common.rs`
- 27 integration tests in `crates/ghostlink-core/tests/integration.rs`

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
