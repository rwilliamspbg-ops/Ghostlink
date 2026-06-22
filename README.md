# Ghost-Link 🚀

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![CI](https://github.com/rwilliamspbg-ops/Ghostlink/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/rwilliamspbg-ops/Ghostlink/actions/workflows/ci.yml)
[![Benchmarks](https://github.com/rwilliamspbg-ops/Ghostlink/actions/workflows/benchmarks.yml/badge.svg?branch=main)](https://github.com/rwilliamspbg-ops/Ghostlink/actions/workflows/benchmarks.yml)
[![Rust](https://img.shields.io/badge/Rust-1.70+-orange.svg)](https://www.rust-lang.org)

Ghost-Link is an open-source, zero-config LAN fabric that turns spare local GPUs into a shared execution surface for large-model inference and training. The system moves tensors directly over the local wire without forcing workloads through heavy orchestration layers or cloud subscriptions.

## Features

- **Zero-Copy Ring Buffers**: SPSC (Single Producer / Single Consumer) DMA-style hand-off with backpressure
- **Binary Protocol**: Custom Layer-2 discovery protocol with EtherType `0x88B5` and CRC32 checksums
- **Thread-Safe Cluster State**: Lock-free node tracking with live metrics collection
- **Adaptive Quantization**: Automatic fallback to 8-bit or 4-bit when delivery health degrades
- **Fault Tolerance**: Heartbeat monitoring, automatic recovery, load rebalancing
- **Terminal Dashboard**: Live ASCII display with ratatui integration for production use

## Quick Start

```bash
# Build the workspace
cargo build --workspace

# Run unit tests
cargo test --workspace

# Generate layer placement plan
cargo run -p ghost-link -- plan

# Join cluster with node ID
cargo run -p ghost-link -- join node-02

# Display ASCII dashboard
cargo run -p ghost-link -- dashboard
```

## Architecture

```
Ghostlink/
├── crates/
│   ├── ghostlink-core/  # Shared runtime primitives
│   │   ├── ring.rs          # Zero-copy SPSC ring buffer
│   │   ├── protocol.rs      # Binary protocol with CRC32
│   │   ├── cluster.rs       # Thread-safe node state
│   │   ├── planning.rs      # Layer assignment + fault tolerance
│   │   ├── health.rs        # Network health monitoring
│   │   ├── load_balance.rs  # Tensor distribution
│   │   ├── xdp.rs           # AF_XDP/eBPF integration (Linux)
│   │   └── dashboard.rs     # Terminal UI with ratatui
│   └── ghost-link/          # CLI demo entrypoint
├── tests/                    # Integration test suite
├── docs/                     # Documentation
│   ├── ARCHITECTURE.md       # Detailed design docs
│   ├── TROUBLESHOOTING.md    # Common issues and solutions
│   └── EXAMPLES.md           # Usage examples
└── README.md
```

## Design Principles

1. **Zero-Copy / Low Latency**: Prioritize zero-copy primitives, raw memory alignment, and minimal allocation in the hot path.
2. **No Bloat**: Avoid heavy cloud-native orchestration frameworks. This is a bare-metal, raw frame LAN fabric.
3. **Idiomatic Rust**: Enforce strict type safety, correct lifetime management for shared ring buffers, and explicit error handling without relying on unwrap().

## Documentation

- [ARCHITECTURE.md](docs/ARCHITECTURE.md) - Detailed design documentation
- [TROUBLESHOOTING.md](docs/TROUBLESHOOTING.md) - Common issues and solutions
- [EXAMPLES.md](docs/EXAMPLES.md) - Usage examples and best practices
- [CONTRIBUTING.md](CONTRIBUTING.md) - Guidelines for contributing

## Examples

### Layer Placement Plan

```bash
$ cargo run -p ghost-link -- plan
Ghost-Link Layer Placement Plan

================================

- node-a => layers 0-23 (24.0 GB)
- node-b => layers 23-32 (9.0 GB)

Adaptive Quantization Trigger:

delivery_ratio=0.98 => None
delivery_ratio=0.90 => Int8
delivery_ratio=0.75 => Int4
```

### Join Cluster

```bash
$ cargo run -p ghost-link -- join node-02
Broadcasting Ghost-Link Join Frame

====================================

Frame Size: 62 bytes
EtherType: 0x88B5

Node Information:

  ID: node-02
  VRAM: 12.0 GB
  System Memory: 32.0 GB
  Compute Capability: 8.6
```

### ASCII Dashboard

```bash
$ cargo run -p ghost-link -- dashboard
+───────────────────────────────────────────────────────────────+
| GHOST-LINK CLUSTER DASHBOARD               [STATUS: ACTIVE] |
+───────────────────────────────────────────────────────────────+
| Ring Buffer Fill:  63%                    Gradient Steps:    42 |
+───────────────────────────────────────────────────────────────+
| NODE-01 (RTX4090) [██████████████████░░] 22.4 / 24.0 GB VRAM |
| >>> Streaming Layers   0-24 >>> [AF_XDP:    9.8 Gbps /   1.2μs] |
+───────────────────────────────────────────────────────────────+
```

## Testing

### Unit Tests

```bash
# Run all unit tests
cargo test --workspace

# Run specific crate tests
cargo test --package ghostlink-core
cargo test --package ghost-link

# Run with verbose output
cargo test --workspace -- --nocapture
```

### Integration Tests

```bash
# Run integration tests
cargo test --test integration

# Run with coverage report
cargo tarpaulin --workspace
```

## Performance Baseline

Measured on `x86_64` with Criterion (`cargo bench -p ghostlink-core --bench criterion -- --warm-up-time 1 --measurement-time 1`). GitHub Actions uploads the raw benchmark logs and Criterion report directory as artifacts.

### Latest Criterion Run

| Benchmark | Latency | Throughput |
|---|---|---|
| Ring buffer push+pop round-trip (ST) | ~2.89-3.03 ns | ~330 M ops/s |
| Ring buffer push only (ST, full=drain) | ~1.89-1.98 ns | ~518 M ops/s |
| Protocol: `DiscoveryFrame` encode | ~100.04-103.54 ns | ~9.7 M ops/s |
| Protocol: `DiscoveryFrame` decode | ~102.50-103.81 ns | ~9.7 M ops/s |
| Protocol: encode + decode round-trip | ~210.21-219.03 ns | ~4.6 M ops/s |
| Planning: 33 layers across 2 nodes | ~164.90-170.18 ns | ~5.9 M ops/s |
| Planning: 80 layers across 8 nodes | ~344.28-354.76 ns | ~2.9 M ops/s |
| Cluster: `register` node (update path) | ~179.57-186.40 ns | ~5.4 M ops/s |
| Cluster: `nodes_snapshot()` (10 nodes) | ~9.78-10.28 ns | ~97 M ops/s |
| Cluster: `total_vram_gb()` (10 nodes) | ~1.59-1.77 ns | ~600 M ops/s |

Run the baseline yourself:

```bash
# Build and run the Criterion benchmark harness
cargo bench -p ghostlink-core --bench criterion -- --warm-up-time 1 --measurement-time 1
```

> **Note**: The SPSC cross-thread figure includes OS scheduling overhead from `yield_now`. Raw ring buffer latency for the single-threaded path remains sub-3 ns.

Benchmark artifacts are captured automatically in GitHub Actions under the `Benchmarks` workflow.

## Dependencies

### Core Dependencies
- `pin-utils` - Zero-copy pinned allocations
- `crc32fast` - Fast CRC32 checksums
- `arc-swap` - Shared, lock-free snapshot reads
- `thiserror` - Error types
- `tokio` - Async runtime (for health monitoring)
- `ratatui` - Terminal UI
- `anyhow` - Production error handling

### Dev Dependencies
- `tokio-test` - Tokio testing utilities
- `criterion` - Benchmark harness for hot-path regression tracking

## Platform Support

- **Linux**: Full support with AF_XDP/eBPF integration
- **Windows/macOS**: Standard socket implementation (AF_XDP not available)

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines on contributing to the project.

### Areas for Contribution

1. **High Priority**: AF_XDP/eBPF integration (Linux kernel wiring), real network health probes
2. **Medium Priority**: Criterion-based micro-benchmarks, CI/CD pipeline
3. **Nice to Have**: Additional examples, extended documentation

## License

Ghost-Link is released under the MIT License to keep experimentation and enterprise adoption friction low.

## Acknowledgments

This project is inspired by the need for a copy-paste local clustering workflow where workstations, gaming rigs, and rack servers can discover each other, advertise resources, and participate in a shared execution graph with a single command.

---

For more information, see the [documentation](docs/ARCHITECTURE.md).
