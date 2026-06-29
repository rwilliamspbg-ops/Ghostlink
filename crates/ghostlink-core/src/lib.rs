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

pub mod accelerator;
pub mod cluster;
pub mod dashboard;
pub mod discovery;
pub mod health;
pub mod host;
pub mod load_balance;
pub mod planning;
pub mod protocol;
pub mod ring;
pub mod runtime;
pub mod xdp;

// Re-export common types for convenience
pub use accelerator::ExecutionBackend;
pub use cluster::{ClusterState, NodeMetrics, NodeStatus};
pub use discovery::{broadcast_and_collect, UdpDiscoveryConfig, DEFAULT_DISCOVERY_PORT};
pub use host::{
    detect_local_node_resources, detect_runtime_profile, detect_runtime_profile_full,
    detect_runtime_profile_with_mode, AccelerationMode, ProbeMode, RuntimeProfile,
};
pub use planning::{
    assign_layers_sequentially, assign_layers_with_fault_tolerance_and_runtime,
    assign_layers_with_runtime_profile, chunk_assignments_for_workers, select_quantization_mode,
    LayerAssignment, LayerSpec, PlacementPlan, PlanningTuning, QuantizationMode,
};
pub use protocol::NodeResources;
pub use ring::{RingConfig, SpscRingBuffer};
pub use runtime::{
    build_token_schedule, execute_pipeline, execute_pipeline_tcp_loopback,
    execute_pipeline_tcp_loopback_with_config, DeviceKind, ExecutionResult, PipelinePlan,
    StageExecutionStats, StagePlacement, TcpTransportConfig, TokenStep,
};
