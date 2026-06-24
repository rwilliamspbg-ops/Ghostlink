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

# Package-owned integration suite
cargo test -p ghostlink-core --test integration

# Lint all targets, including benches
cargo clippy --workspace --all-targets -- -D warnings

# Run criterion benchmarks
cargo bench -p ghostlink-core --bench criterion
```

## Current Counts

The current validated workspace contains 100 passing tests:

- 66 library tests in `ghostlink-core`
- 7 tests in `crates/ghostlink-core/tests/common.rs`
- 27 integration tests in `crates/ghostlink-core/tests/integration.rs`

## Coverage Status

Coverage is not currently reported by CI and should be treated as unmeasured until a fresh coverage run is published.

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
- Health monitoring still uses synthetic values rather than live network probes
- Tensor migration and dynamic cluster rebalance remain incomplete
