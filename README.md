# Ghost-Link

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![CI](https://github.com/rwilliamspbg-ops/Ghostlink/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/rwilliamspbg-ops/Ghostlink/actions/workflows/ci.yml)
[![Benchmarks](https://github.com/rwilliamspbg-ops/Ghostlink/actions/workflows/benchmarks.yml/badge.svg?branch=main)](https://github.com/rwilliamspbg-ops/Ghostlink/actions/workflows/benchmarks.yml)
[![Rust](https://img.shields.io/badge/Rust-stable-orange.svg)](https://www.rust-lang.org)
[![Tests](https://img.shields.io/badge/Tests-100%20passing-brightgreen)](TESTING.md)
[![Coverage](https://img.shields.io/badge/Coverage-not%20measured-lightgrey)](TESTING.md)

Ghost-Link is an open-source LAN fabric for turning spare local GPUs and CPU hosts into a shared execution surface for large-model inference and training. The project focuses on low-overhead runtime primitives, binary discovery, host-aware autotuning, and runtime-selected execution backends rather than heavy orchestration.

## Features

- Zero-copy SPSC ring buffers with backpressure handling
- Binary Layer-2 discovery protocol with CRC32 validation
- Thread-safe cluster state with live metrics and fault tracking
- Runtime-aware planning, load balancing, and health thresholds
- Fast and full hardware probe modes with cached host detection
- Runtime-selected execution backends for GPU, AVX-512, AVX2, NEON, and generic CPU paths
- Terminal dashboard and CLI demo commands

## Install

```bash
# Install stable Rust
curl https://sh.rustup.rs -sSf | sh -s -- -y
. "$HOME/.cargo/env"

# Clone the repository
git clone https://github.com/rwilliamspbg-ops/Ghostlink.git
cd Ghostlink

# Build the workspace
cargo build --workspace
```

## Usage

```bash
# Run the full workspace test suite
cargo test --workspace

# Run the package-owned integration suite
cargo test -p ghostlink-core --test integration

# Generate a layer placement plan
cargo run -p ghost-link -- plan

# Emit a join frame for a specific node ID
cargo run -p ghost-link -- join node-02

# Render the sample dashboard
cargo run -p ghost-link -- dashboard

# Detect the local runtime profile using the fast cached probe path
cargo run -p ghost-link -- probe local-node fast

# Detect the local runtime profile using the deeper full probe path
cargo run -p ghost-link -- probe local-node full
```

Fast mode uses cheap local signals and a short-lived cache. Full mode enables deeper hardware probing, including external tools when they are available on the host.

## Probe Modes

- `fast`: cheap local detection intended for frequent runtime use
- `full`: deeper hardware inspection intended for operator diagnostics

If the host does not provide tools such as `nvidia-smi` or `lspci`, full mode falls back to the same sysfs and local signals available to fast mode.

## Repository Layout

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
│   │   │   ├── lib.rs
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
├── docs/
├── TESTING.md
└── README.md
```

## Current Validation

The current workspace validation passes:

- `cargo test --workspace`
- `cargo clippy --workspace --all-targets -- -D warnings`

That currently covers 100 passing tests across library and package-owned integration targets.

## Benchmark Notes

Recent measured results in this environment include:

- `autotune/detect_runtime_profile_fast`: about `187-217 ns`
- `autotune/detect_runtime_profile_full`: about `192-203 us`
- `planning/80_layers_8_nodes_autotuned`: about `877-954 ns`
- `autotune/load_balance_80_layers_autotuned`: about `2.42-2.68 us`

The fast probe path is intended for frequent runtime use. The full probe path is intentionally slower.

## Limitations

- AF_XDP/eBPF remains Linux-specific and is not backed by kernel integration tests yet
- Full hardware probing depends on the tools and kernel interfaces available on the host
- Health monitoring still uses synthetic probe inputs rather than real network latency traffic

## Documentation

- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
- [docs/EXAMPLES.md](docs/EXAMPLES.md)
- [docs/TROUBLESHOOTING.md](docs/TROUBLESHOOTING.md)
- [TESTING.md](TESTING.md)
- [CONTRIBUTING.md](CONTRIBUTING.md)

## License

Ghost-Link is released under the MIT License.
