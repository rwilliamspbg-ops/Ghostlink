# Ghost-Link Architecture Documentation

## Overview

Ghost-Link is an open-source, zero-config LAN fabric designed to pool local consumer GPUs into a single shared execution surface for large-model inference and training. The system moves tensors directly over the local wire without forcing workloads through heavy orchestration layers or cloud subscriptions.

## Core Design Principles

1. **Zero-Copy / Low Latency**: Prioritize zero-copy primitives, raw memory alignment, and minimal allocation in the hot path.
2. **No Bloat**: Avoid heavy cloud-native orchestration frameworks. This is a bare-metal, raw frame LAN fabric.
3. **Idiomatic Rust**: Enforce strict type safety, correct lifetime management for shared ring buffers, and explicit error handling without relying on unwrap().

## Workspace Structure

```
Ghostlink/
├── Cargo.toml                          # Workspace manifest
├── crates/
│   ├── ghostlink-core/                 # Shared runtime primitives
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs                  # Module exports
│   │       ├── ring.rs                 # Zero-copy SPSC ring buffer
│   │       ├── protocol.rs             # Binary protocol with CRC32
│   │       ├── cluster.rs              # Thread-safe node state
│   │       ├── planning.rs             # Layer assignment + fault tolerance
│   │       ├── health.rs               # Network health monitoring
│   │       ├── load_balance.rs         # Tensor distribution
│   │       ├── xdp.rs                  # AF_XDP/eBPF integration (Linux)
│   │       └── dashboard.rs            # Terminal UI with ratatui
│   └── ghost-link/                     # CLI demo entrypoint
│       ├── Cargo.toml
│       └── src/
│           └── main.rs                  # CLI commands
├── tests/                              # Integration test suite
│   └── integration.rs
├── docs/                               # Documentation
│   ├── ARCHITECTURE.md                 # This file
│   ├── TROUBLESHOOTING.md              # Common issues
│   └── EXAMPLES.md                     # Usage scenarios
└── README.md
```

## Component Details

### Ring Buffer (`ring.rs`)

The zero-copy SPSC (Single Producer / Single Consumer) ring buffer is the foundation for DMA-style hand-off between nodes.

**Key Features:**
- Pinned allocations using `pin-utils` crate
- Proper memory ordering with `AtomicUsize` and release/acquire semantics
- Backpressure thresholds at 70% capacity
- Graceful overflow handling with backpressure signals
- Thread-safe for single producer / single consumer pattern

**Memory Layout:**
```rust
pub struct SpscRingBuffer<T> {
    buffer: UnsafeCell<MaybeUninit<[MaybeUninit<T>; CAPACITY]>>,
    head: AtomicUsize,      // Producer writes here
    tail: AtomicUsize,      // Consumer reads here
    overflow_count: AtomicUsize,
    empty_count: AtomicUsize,
    config: RingConfig,
}
```

### Protocol (`protocol.rs`)

Custom Layer-2 discovery protocol using a dedicated EtherType (`0x88B5`) to encode join/discovery/attestation frames.

**Key Features:**
- Fixed-width binary fields for zero-copy parsing
- CRC32 checksums for frame integrity
- Sequence numbers and versioning for ordering
- Support for multiple frame kinds: Discovery, Join, Attestation, HealthCheck, ResourceAdvert

**Frame Structure:**
```rust
pub struct FrameHeader {
    ether_type: u16,      // 0x88B5 (Ghost-Link EtherType)
    kind: u8,             // FrameKind enum
    version: u8,          // Protocol version
    crc: u32,             // CRC32 of payload
}

pub struct DiscoveryFrame {
    kind: FrameKind,
    node: NodeResources,  // VRAM, system memory, compute capability
}
```

### Cluster State (`cluster.rs`)

Thread-safe cluster state tracking discovered node capabilities and live metrics.

**Key Features:**
- Lock-free HashMap for node resources
- Live metrics collection (latency, delivery ratio, throughput)
- Fault detection with heartbeat timeouts
- Node status enumeration: Active, Degraded, Failed

**Metrics Structure:**
```rust
pub struct NodeMetrics {
    pub status: NodeStatus,
    pub vram_gb: f32,
    pub system_memory_gb: f32,
    pub compute_capability: String,
    pub avg_latency_us: f32,
    pub delivery_ratio: f32,
    pub throughput_gbps: f32,
    pub used_vram_gb: f32,
}
```

### Planning (`planning.rs`)

Greedy layer splitting across nodes based on VRAM capacity with adaptive quantization.

**Key Features:**
- Sequential greedy layer assignment
- Adaptive quantization trigger (`select_quantization_mode`)
- Load balancing and fault tolerance integration
- Quantization modes: None (full precision), Int8, Int4

**Quantization Thresholds:**
- `DELIVERY_RATIO_INT8_THRESHOLD = 0.95`
- `DELIVERY_RATIO_INT4_THRESHOLD = 0.80`

### Health Monitoring (`health.rs`)

Network health monitoring with ping/pong latency tracking and automatic quantization fallback.

**Key Features:**
- Periodic health checks every 5 seconds
- Delivery ratio monitoring per node
- Automatic quantization fallback triggers
- Fault detection and recovery

### Load Balancing (`load_balance.rs`)

Tensor distribution across nodes with dynamic load shedding.

**Key Features:**
- VRAM-aware layer distribution
- Deadlock prevention with timeout
- Load rebalancing based on metrics
- Transfer planning between overloaded/underloaded nodes

### XDP Integration (`xdp.rs`)

AF_XDP/eBPF socket integration for Linux environments.

**Key Features:**
- Raw socket binding with AF_XDP
- EtherType filtering (0x88B5)
- Frame reception loop with zero-copy buffers
- eBPF program loading helpers

### Dashboard (`dashboard.rs`)

Terminal UI with ratatui integration and ASCII fallback.

**Key Features:**
- Live cluster metrics display
- Node status indicators
- Streaming layer visualization
- Operator controls (restart, reassign)

## Data Flow

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│   Node A    │────▶│   Ring Buf  │◀────│   Node B    │
│  (Producer) │     │ (Zero-Copy) │     │ (Consumer)  │
└─────────────┘     └─────────────┘     └─────────────┘
      │                   │                   │
      ▼                   ▼                   ▼
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│ Discovery   │────▶│ Protocol    │◀────│ Attestation │
│  Frame (0x88B5)│  │ Encoding +  │     │ Frame       │
└─────────────┘     │ CRC32 Check │     └─────────────┘
                    └─────────────┘
```

## Build and Test

```bash
# Build the workspace
cargo build --workspace

# Run unit tests
cargo test --workspace

# Run integration tests
cargo test --test integration

# Run CLI demo
cargo run -p ghost-link -- plan
cargo run -p ghost-link -- join node-02
cargo run -p ghost-link -- dashboard
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

## License

Ghost-Link is released under the MIT License to keep experimentation and enterprise adoption friction low.
