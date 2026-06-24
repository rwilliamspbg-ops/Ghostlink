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
│   │   │   ├── health.rs
│   │   │   ├── host.rs
│   │   │   ├── load_balance.rs
│   │   │   ├── planning.rs
│   │   │   ├── protocol.rs
│   │   │   ├── ring.rs
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

## Validation Commands

```bash
cargo test --workspace
cargo test -p ghostlink-core --test integration
cargo clippy --workspace --all-targets -- -D warnings
```
