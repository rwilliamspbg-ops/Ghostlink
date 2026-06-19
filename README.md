# Ghost-Link 🚀

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
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

## Dependencies

### Core Dependencies
- `pin-utils` - Zero-copy pinned allocations
- `crc32fast` - Fast CRC32 checksums
- `thiserror` - Error types
- `tokio` - Async runtime (for health monitoring)
- `ratatui` - Terminal UI
- `anyhow` - Production error handling

### Dev Dependencies
- `tokio-test` - Tokio testing utilities

## Platform Support

- **Linux**: Full support with AF_XDP/eBPF integration
- **Windows/macOS**: Standard socket implementation (AF_XDP not available)

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines on contributing to the project.

### Areas for Contribution

1. **High Priority**: AF_XDP/eBPF integration, network health monitoring, load balancing
2. **Medium Priority**: Documentation, testing, error handling
3. **Nice to Have**: Benchmarks, examples, CI/CD improvements

## License

Ghost-Link is released under the MIT License to keep experimentation and enterprise adoption friction low.

## Acknowledgments

This project is inspired by the need for a copy-paste local clustering workflow where workstations, gaming rigs, and rack servers can discover each other, advertise resources, and participate in a shared execution graph with a single command.

---

For more information, see the [documentation](docs/ARCHITECTURE.md).
