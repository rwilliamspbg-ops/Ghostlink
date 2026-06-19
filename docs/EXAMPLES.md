# Ghost-Link Usage Examples

## Quick Start

### 1. Basic CLI Commands

```bash
# Show help
cargo run -p ghost-link -- help

# Generate layer placement plan
cargo run -p ghost-link -- plan

# Join cluster with node ID
cargo run -p ghost-link -- join node-01

# Display ASCII dashboard
cargo run -p ghost-link -- dashboard
```

### 2. Running Unit Tests

```bash
# Run all tests
cargo test --workspace

# Run specific crate tests
cargo test --package ghostlink-core
cargo test --package ghost-link

# Run integration tests
cargo test --test integration

# Run with verbose output
cargo test --workspace -- --nocapture
```

## Layer Placement Examples

### Example 1: Simple Two-Node Cluster

```rust
use ghostlink_core::{cluster::ClusterState, planning::{assign_layers_sequentially, LayerSpec}};
use ghostlink_core::protocol::DiscoveryFrame;

fn main() {
    // Create cluster with two nodes
    let mut cluster = ClusterState::new();
    
    // Register node A (RTX 4090)
    cluster.register(ghostlink_core::protocol::NodeResources::new(
        "gpu-node-1",
        24.0,           // VRAM in GB
        64.0,           // System memory in GB
        "9.0",          // CUDA compute capability
        Some("NVIDIA GeForce RTX 4090".to_string()),
    ));
    
    // Register node B (RTX 3080)
    cluster.register(ghostlink_core::protocol::NodeResources::new(
        "gpu-node-2",
        12.0,
        32.0,
        "8.6",
        Some("NVIDIA GeForce RTX 3080".to_string()),
    ));
    
    // Create layer specs (e.g., Llama-7B has ~33 layers)
    let layers: Vec<LayerSpec> = (0..33)
        .map(|index| LayerSpec { 
            index, 
            vram_gb: 1.0,
            num_weights: 0, 
        })
        .collect();
    
    // Assign layers to nodes
    let assignments = assign_layers_sequentially(&cluster.nodes(), &layers)
        .expect("Failed to assign layers");
    
    println!("Layer Assignment Plan:");
    for assignment in &assignments {
        println!("  {} => layers {}-{} ({:.1} GB)", 
            assignment.node_id,
            assignment.start_layer,
            assignment.end_layer,
            assignment.used_vram_gb);
    }
}
```

**Output:**
```
Layer Assignment Plan:
  gpu-node-1 => layers 0-23 (24.0 GB)
  gpu-node-2 => layers 23-32 (9.0 GB)
```

### Example 2: Adaptive Quantization

```rust
use ghostlink_core::{planning::select_quantization_mode};

fn main() {
    // Simulate different delivery ratios and select quantization mode
    
    println!("Quantization Mode Selection:\n");
    
    for ratio in [0.98_f32, 0.90, 0.75] {
        let mode = select_quantization_mode(ratio);
        match mode {
            ghostlink_core::planning::QuantizationMode::None => {
                println!("delivery_ratio={:.2} => Full Precision (no quantization)", ratio);
            }
            ghostlink_core::planning::QuantizationMode::Int8 => {
                println!("delivery_ratio={:.2} => 8-bit Quantized", ratio);
            }
            ghostlink_core::planning::QuantizationMode::Int4 => {
                println!("delivery_ratio={:.2} => 4-bit Quantized", ratio);
            }
        }
    }
}
```

**Output:**
```
Quantization Mode Selection:

delivery_ratio=0.98 => Full Precision (no quantization)
delivery_ratio=0.90 => 8-bit Quantized
delivery_ratio=0.75 => 4-bit Quantized
```

## Discovery Frame Examples

### Example 1: Creating and Encoding a Discovery Frame

```rust
use ghostlink_core::protocol::{DiscoveryFrame, FrameKind, NodeResources};

fn main() {
    // Create discovery frame
    let frame = DiscoveryFrame {
        kind: FrameKind::Join,
        node: NodeResources::new(
            "my-gpu-node",
            24.0,           // VRAM in GB
            64.0,           // System memory in GB
            "9.0",          // CUDA compute capability
            Some("NVIDIA GeForce RTX 4090".to_string()),
        ),
    };
    
    let encoded = frame.encode();
    println!("Encoded frame size: {} bytes", encoded.len());
    
    // Decode the frame back
    let decoded = DiscoveryFrame::decode(&encoded).unwrap();
    println!("Decoded node ID: {}", decoded.node.id);
    println!("Decoded VRAM: {:.1} GB", decoded.node.vram_gb);
}
```

### Example 2: Broadcasting to Multiple Nodes

```rust
use ghostlink_core::protocol::{DiscoveryFrame, FrameKind, NodeResources};
use std::net::{UdpSocket, SocketAddr};

fn main() {
    let socket = UdpSocket::bind("0.0.0.0:9876").unwrap();
    
    // Define target nodes
    let targets = vec![
        "192.168.1.100:9876",
        "192.168.1.101:9876",
        "192.168.1.102:9876",
    ];
    
    // Create discovery frame
    let frame = DiscoveryFrame {
        kind: FrameKind::Discovery,
        node: NodeResources::new(
            "broadcast-node",
            48.0,
            128.0,
            "9.0",
            Some("NVIDIA GeForce RTX 4090".to_string()),
        ),
    };
    
    let encoded = frame.encode();
    
    // Broadcast to all targets
    for target in &targets {
        if let Ok(addr) = SocketAddr::from_str(target) {
            socket.send_to(&encoded, addr).unwrap();
            println!("Broadcasted to {}", target);
        }
    }
}
```

## Ring Buffer Examples

### Example 1: Basic Producer/Consumer Pattern

```rust
use ghostlink_core::ring::{SpscRingBuffer, RingConfig};
use std::sync::Arc;
use std::thread;

fn main() {
    let ring = Arc::new(SpscRingBuffer::<i32>::new(RingConfig::default()));
    let producer_ring = Arc::clone(&ring);
    let consumer_ring = Arc::clone(&ring);
    
    // Producer thread
    let producer = thread::spawn(move || {
        for value in 0..1_000 {
            loop {
                if producer_ring.push(value).is_ok() {
                    break;
                }
                std::thread::yield_now(); // Backpressure handling
            }
        }
    });
    
    // Consumer thread
    let consumer = thread::spawn(move || {
        let mut sum: i32 = 0;
        while let Some(value) = consumer_ring.pop() {
            sum += value;
        }
        println!("Sum of consumed values: {}", sum);
        sum
    });
    
    producer.join().unwrap();
    consumer.join().unwrap();
}
```

### Example 2: High-Throughput Ring Buffer

```rust
use ghostlink_core::ring::{SpscRingBuffer, RingConfig};
use std::sync::Arc;
use std::thread;
use std::time::{Instant, Duration};

fn main() {
    // Create ring with high capacity
    let ring = Arc::new(SpscRingBuffer::<f32>::new(RingConfig {
        capacity: 8192,
        backpressure_threshold: 5734, // 70% of 8192
    }));
    
    let producer_ring = Arc::clone(&ring);
    let consumer_ring = Arc::clone(&ring);
    
    // Producer thread
    let producer = thread::spawn(move || {
        let start = Instant::now();
        for i in 0..1_000_000 {
            loop {
                if producer_ring.push(i as f32).is_ok() {
                    break;
                }
                std::thread::yield_now();
            }
        }
        println!("Producer finished in {:.2}s", start.elapsed().as_secs_f64());
    });
    
    // Consumer thread with timing
    let consumer = thread::spawn(move || {
        let start = Instant::now();
        let mut count = 0;
        while count < 1_000_000 {
            if let Some(value) = consumer_ring.pop() {
                count += 1;
            } else {
                std::thread::yield_now();
            }
        }
        println!("Consumer finished in {:.2}s", start.elapsed().as_secs_f64());
    });
    
    producer.join().unwrap();
    consumer.join().unwrap();
}
```

## Cluster Management Examples

### Example 1: Registering Multiple Nodes

```rust
use ghostlink_core::cluster::{ClusterState, NodeResources};

fn main() {
    let mut cluster = ClusterState::new();
    
    // Register GPU nodes
    cluster.register(NodeResources::new(
        "workstation-1",
        24.0,
        64.0,
        "8.9",
        Some("NVIDIA GeForce RTX 4090".to_string()),
    ));
    
    cluster.register(NodeResources::new(
        "gaming-pc-1",
        12.0,
        32.0,
        "8.6",
        Some("NVIDIA GeForce RTX 3080".to_string()),
    ));
    
    cluster.register(NodeResources::new(
        "server-node-1",
        48.0,
        128.0,
        "9.0",
        Some("NVIDIA A100".to_string()),
    ));
    
    println!("Cluster registered {} nodes", cluster.nodes().len());
    println!("Total VRAM: {:.1} GB", cluster.total_vram_gb());
    println!("System memory: {:.1} GB", cluster.total_system_memory_gb());
}
```

### Example 2: Health Monitoring

```rust
use ghostlink_core::{cluster::ClusterState, health::{NetworkHealthMonitor, HealthConfig}};
use std::sync::Arc;

fn main() {
    let mut cluster = ClusterState::new();
    
    // Register nodes
    for i in 0..3 {
        cluster.register(NodeResources::new(
            format!("node-{}", i),
            24.0,
            64.0,
            "8.9".to_string(),
            None,
        ));
    }
    
    // Create health monitor
    let cluster = Arc::new(cluster);
    let monitor = NetworkHealthMonitor::new(cluster.clone(), HealthConfig::default());
    
    // Run health check
    monitor.check_all();
    
    // Get health report
    println!("{}", monitor.get_health_summary());
}
```

## Dashboard Examples

### Example 1: ASCII Dashboard

```rust
use ghostlink_core::{cluster::ClusterState, dashboard::{Dashboard, NodeMetrics}};
use ghostlink_core::protocol::NodeResources;

fn main() {
    let cluster = ClusterState::new();
    
    // Register nodes
    cluster.register(NodeResources::new("NODE-01", 24.0, 64.0, "8.9".to_string()));
    cluster.register(NodeResources::new("NODE-02", 12.0, 32.0, "8.6".to_string()));
    
    // Create metrics
    let nodes = vec![
        NodeMetrics {
            name: "NODE-01".into(),
            gpu_name: Some("RTX4090".into()),
            used_vram_gb: 22.4,
            total_vram_gb: 24.0,
            streaming_layers: Some((0, 24)),
            af_xdp_gbps: 9.8,
            latency_micros: 1.2,
        },
        NodeMetrics {
            name: "NODE-02".into(),
            gpu_name: Some("RTX3080".into()),
            used_vram_gb: 7.2,
            total_vram_gb: 12.0,
            streaming_layers: None,
            af_xdp_gbps: 0.0,
            latency_micros: 0.0,
        },
    ];
    
    let dashboard = Dashboard::new(cluster, 63, 42, nodes);
    println!("{}", dashboard.render_ascii());
}
```

### Example 2: Terminal Application

```rust
// This would use ratatui for interactive terminal UI
use ghostlink_core::dashboard::{DashboardState, TerminalApp};

fn main() {
    let cluster = ClusterState::new();
    let state = DashboardState::new(cluster);
    let app = TerminalApp::new(state);
    app.run(); // Runs in terminal with live updates
}
```

## Performance Benchmarking

### Example 1: Ring Buffer Throughput Test

```rust
use ghostlink_core::ring::{SpscRingBuffer, RingConfig};
use std::sync::Arc;
use std::thread;
use std::time::{Instant, Duration};

fn benchmark_ring_buffer() {
    let ring = Arc::new(SpscRingBuffer::<f32>::new(RingConfig {
        capacity: 8192,
        backpressure_threshold: 5734,
    }));
    
    let producer_ring = Arc::clone(&ring);
    let consumer_ring = Arc::clone(&ring);
    
    let num_iterations = 100_000;
    
    // Producer
    let producer = thread::spawn(move || {
        let start = Instant::now();
        for _ in 0..num_iterations {
            loop {
                if producer_ring.push(1.0).is_ok() {
                    break;
                }
                std::thread::yield_now();
            }
        }
        start.elapsed()
    });
    
    // Consumer
    let consumer = thread::spawn(move || {
        let start = Instant::now();
        for _ in 0..num_iterations {
            if let Some(_) = consumer_ring.pop() {
                // Process element
            } else {
                std::thread::yield_now();
            }
        }
        start.elapsed()
    });
    
    producer.join().unwrap();
    consumer.join().unwrap();
}

fn main() {
    let duration = benchmark_ring_buffer();
    println!("Ring buffer throughput: {:.1} MB/s", 
        (100_000 * std::mem::size_of::<f32>() as f64 / 1_000_000.0) 
            / duration.as_secs_f64());
}
```

## Best Practices

### 1. Always Handle Backpressure

```rust
// Good: Implements backpressure
loop {
    if ring.push(value).is_ok() {
        break;
    }
    std::thread::yield_now(); // Allow consumer to catch up
}

// Bad: Can cause overflow
ring.push(value).unwrap(); // Panics on failure
```

### 2. Validate Protocol Frames

```rust
// Good: Validates before processing
if let Ok(frame) = DiscoveryFrame::decode(&bytes) {
    process_frame(frame);
} else {
    tracing::warn!("Failed to decode frame");
}

// Bad: Assumes valid input
let frame = DiscoveryFrame::decode(&bytes).unwrap(); // Panics on error
```

### 3. Monitor Cluster Health

```rust
// Good: Regular health checks
monitor.check_all();
let summary = monitor.get_health_summary();
if summary.contains("Failed") {
    tracing::warn!("Cluster has failed nodes");
}

// Bad: No health monitoring
// Nodes can fail silently
```

### 4. Use Adaptive Quantization

```rust
// Good: Falls back to quantization when needed
let mode = select_quantization_mode(delivery_ratio);
if mode != QuantizationMode::None {
    apply_quantization(mode);
}

// Bad: Always uses full precision
apply_full_precision(); // May run out of VRAM
```

## Next Steps

For more advanced usage, see:

- `docs/ARCHITECTURE.md` for detailed component documentation
- `docs/TROUBLESHOOTING.md` for common issues and solutions
- Integration tests in `tests/integration.rs` for example patterns

Happy clustering! 🚀
