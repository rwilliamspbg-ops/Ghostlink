# Ghost-Link Architecture

## Overview

Ghost-Link is a Rust workspace for low-overhead cluster discovery, host profiling, planning, and load distribution across local compute nodes.

## Workspace Structure

```text
Ghostlink/
├── crates/
│   ├── ghostlink-core/
│   │   ├── src/
│   │   │   ├── accelerator.rs
│   │   │   ├── cluster.rs
│   │   │   ├── dashboard.rs
│   │   │   ├── discovery.rs
│   │   │   ├── health.rs
│   │   │   ├── host.rs
│   │   │   ├── load_balance.rs
│   │   │   ├── planning.rs
│   │   │   ├── protocol.rs
│   │   │   ├── ring.rs
│   │   │   ├── runtime.rs
│   │   │   └── xdp.rs
│   │   └── tests/
│   │       ├── common.rs
│   │       └── integration.rs
│   └── ghost-link/
│       └── src/main.rs
├── benches/
└── docs/
```

## Main Components

## Command Architecture Decision

Ghost-Link uses `crates/ghost-link/src/main.rs` as the single source of truth for CLI command parsing and execution.

- Legacy duplicate command modules under `crates/ghost-link/src/cli/` were retired.
- Legacy duplicate API stub handlers under `crates/ghost-link/src/api/` were retired.
- This avoids drift between parallel command surfaces and keeps behavior and tests aligned to one execution path.

To prevent regressions, CI runs `scripts/verify_no_stub_todos.sh` and fails if unresolved `TODO: Implement actual` markers are reintroduced in `crates/ghost-link/src`.

### `host.rs`

Builds a `RuntimeProfile` for the current machine.

- `fast` probe mode is intended for frequent runtime use
- `full` probe mode enables deeper inspection when available
- fast mode uses a short-lived cache
- full mode can use sysfs and external tools such as `nvidia-smi` or `lspci`

### `accelerator.rs`

Maps the runtime profile to an execution backend.

- GPU staged path
- AVX-512 path
- AVX2 path
- NEON path
- generic scalar fallback

### `planning.rs`

Computes layer placement and chunks work according to runtime-aware tuning.

### `load_balance.rs`

Computes distribution plans and autotuned rebalance settings based on the runtime profile.

### `health.rs`

Applies runtime-aware health thresholds and fault detection settings.

- computes health from collected node metrics (latency, delivery ratio)
- marks fresh nodes as `Unknown` until samples are available
- folds heartbeat timeout into failure decisions
- keeps cluster node status aligned with health outcomes

## Validation Commands

```bash
cargo test --workspace
cargo test -p ghostlink-core --test integration
cargo clippy --workspace --all-targets -- -D warnings
python3 scripts/verify_hf_models.py
```
