//! Ghost-Link Core Library
//!
//! A zero-config LAN fabric that turns spare local GPUs into a shared execution surface
//! for large-model inference and training.
//!
//! # Features
//! - Zero-copy SPSC ring buffers for DMA-style hand-off
//! - Binary protocol with CRC32 checksums for frame integrity
//! - Thread-safe cluster state with metrics collection
//! - Greedy layer assignment with fault tolerance
//! - AF_XDP/eBPF socket integration (Linux)
//! - Network health monitoring
//! - Load balancing and tensor distribution
//!
//! # Architecture
//! ```text
//! crates/
//! ├── ghostlink-core/  # Shared runtime primitives
//! │   ├── ring.rs          # Zero-copy SPSC ring buffer
//! │   ├── protocol.rs      # Binary protocol with CRC32
//! │   ├── cluster.rs       # Thread-safe node state
//! │   ├── planning.rs      # Layer assignment + fault tolerance
//! │   ├── health.rs        # Network health monitoring
//! │   ├── load_balance.rs  # Tensor distribution
//! │   ├── xdp.rs           # AF_XDP/eBPF integration (Linux)
//! │   └── dashboard.rs     # Terminal UI with ratatui
//! └── ghost-link/          # CLI demo entrypoint
//! ```

pub mod cluster;
pub mod dashboard;
pub mod health;
pub mod load_balance;
pub mod planning;
pub mod protocol;
pub mod ring;
pub mod xdp;

// Re-export common types for convenience
pub use cluster::{ClusterState, NodeMetrics, NodeStatus};
pub use planning::{
    assign_layers_sequentially, select_quantization_mode, LayerAssignment, LayerSpec,
    PlacementPlan, QuantizationMode,
};
pub use protocol::NodeResources;
pub use ring::{RingConfig, SpscRingBuffer};
