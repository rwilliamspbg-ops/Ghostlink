use std::sync::Arc;
use std::thread;
/// Performance Baseline for Ghost-Link Primitives
use std::time::Instant;

use ghostlink_core::{
    accelerator::ExecutionBackend,
    cluster::ClusterState,
    host::{
        detect_runtime_profile, detect_runtime_profile_with_mode, AccelerationMode, ProbeMode,
        RuntimeProfile,
    },
    load_balance::LoadBalancer,
    planning::{assign_layers_sequentially, LayerSpec},
    protocol::{DiscoveryFrame, FrameKind, NodeResources},
    ring::{RingConfig, SpscRingBuffer},
};

fn bench(name: &str, iters: u64, mut f: impl FnMut()) -> f64 {
    for _ in 0..1000_u64.min(iters / 10) {
        f();
    }
    let start = Instant::now();
    for _ in 0..iters {
        f();
    }
    let elapsed = start.elapsed();
    let ns = elapsed.as_nanos() as f64 / iters as f64;
    let ops = 1_000_000_000.0 / ns;
    println!("{:<52} {:>10.2} ns/op  {:>14.0} ops/sec", name, ns, ops);
    ns
}

fn main() {
    println!("\nGhost-Link Performance Baseline");
    println!("================================");
    println!(
        "{:<52} {:>10}       {:>14}",
        "Benchmark", "Latency", "Throughput"
    );
    println!("{}", "-".repeat(82));

    // ─── Ring Buffer (single-threaded) ────────────────────────────────────────
    {
        let ring = SpscRingBuffer::<u64>::new(RingConfig::default());
        let mut counter = 0u64;
        bench("ring_buffer: push+pop round-trip (ST)", 1_000_000, || {
            ring.push(counter).ok();
            ring.pop();
            counter = counter.wrapping_add(1);
        });
    }

    {
        let ring = SpscRingBuffer::<u64>::new(RingConfig::default());
        bench("ring_buffer: push only (ST, full=drain)", 1_000_000, || {
            if ring.push(42u64).is_err() {
                ring.pop();
            }
        });
    }

    // ─── SPSC throughput (two threads, 10k items) ────────────────────────────
    {
        let iters = 10_000u64;
        let ring = Arc::new(SpscRingBuffer::<u64>::new(RingConfig::default()));
        let prod = Arc::clone(&ring);
        let cons = Arc::clone(&ring);
        let start = Instant::now();
        let producer = thread::spawn(move || {
            for i in 0..iters {
                loop {
                    if prod.push(i).is_ok() {
                        break;
                    }
                    thread::yield_now();
                }
            }
        });
        let consumer = thread::spawn(move || {
            let mut n = 0u64;
            while n < iters {
                if cons.pop().is_some() {
                    n += 1;
                } else {
                    thread::yield_now();
                }
            }
        });
        producer.join().unwrap();
        consumer.join().unwrap();
        let elapsed = start.elapsed();
        let ns = elapsed.as_nanos() as f64 / iters as f64;
        let ops = 1_000_000_000.0 / ns;
        println!(
            "{:<52} {:>10.2} ns/op  {:>14.0} ops/sec",
            "ring_buffer: SPSC cross-thread (10k items)", ns, ops
        );
    }

    // ─── Protocol ────────────────────────────────────────────────────────────
    let node = NodeResources::new("bench-node", 24.0, 64.0, "8.9", None);
    let frame = DiscoveryFrame {
        kind: FrameKind::Discovery,
        node,
    };

    bench("protocol: DiscoveryFrame encode", 500_000, || {
        let _ = frame.encode();
    });

    let encoded = frame.encode();
    bench("protocol: DiscoveryFrame decode", 500_000, || {
        let _ = DiscoveryFrame::decode(&encoded);
    });

    bench("protocol: encode + decode round-trip", 500_000, || {
        let enc = frame.encode();
        let _ = DiscoveryFrame::decode(&enc);
    });

    // ─── Layer Assignment Planning ───────────────────────────────────────────
    let nodes_2: Vec<_> = (0..2)
        .map(|i| NodeResources::new(format!("node-{}", i), 24.0, 64.0, "8.9", None))
        .collect();
    let layers_33: Vec<LayerSpec> = (0..33)
        .map(|i| LayerSpec {
            index: i,
            vram_gb: 1.0,
            num_weights: 0,
        })
        .collect();
    bench("planning: 33 layers across 2 nodes", 100_000, || {
        let _ = assign_layers_sequentially(&nodes_2, &layers_33);
    });

    let nodes_8: Vec<_> = (0..8)
        .map(|i| NodeResources::new(format!("node-{}", i), 48.0, 128.0, "9.0", None))
        .collect();
    let layers_80: Vec<LayerSpec> = (0..80)
        .map(|i| LayerSpec {
            index: i,
            vram_gb: 1.0,
            num_weights: 0,
        })
        .collect();
    bench("planning: 80 layers across 8 nodes", 100_000, || {
        let _ = assign_layers_sequentially(&nodes_8, &layers_80);
    });

    let runtime_profile = RuntimeProfile {
        node_resources: NodeResources::new("bench-host", 24.0, 64.0, "8.9", None),
        logical_cores: 16,
        recommended_workers: 8,
        acceleration_mode: AccelerationMode::Gpu,
        xdp_supported: true,
        detection_source: String::from("bench"),
        probe_mode: ghostlink_core::ProbeMode::Fast,
    };
    bench(
        "planning: 80 layers across 8 nodes (autotuned)",
        100_000,
        || {
            let _ = ghostlink_core::planning::assign_layers_with_runtime_profile(
                &nodes_8,
                &layers_80,
                &runtime_profile,
            );
        },
    );

    // ─── Cluster State ───────────────────────────────────────────────────────
    let cluster = ClusterState::new();
    bench("cluster: register node (update path)", 100_000, || {
        cluster.register(NodeResources::new("bench", 24.0, 64.0, "8.9", None));
    });

    let cluster2 = ClusterState::new();
    for i in 0..10 {
        cluster2.register(NodeResources::new(
            format!("node-{}", i),
            24.0,
            64.0,
            "8.9",
            None,
        ));
    }
    bench("cluster: nodes() snapshot (10 nodes)", 200_000, || {
        let _ = cluster2.nodes();
    });
    bench("cluster: total_vram_gb() (10 nodes)", 500_000, || {
        let _ = cluster2.total_vram_gb();
    });

    let cluster3 = Arc::new(ClusterState::new());
    for i in 0..8 {
        cluster3.register(NodeResources::new(
            format!("node-{}", i),
            24.0 + (i as f32 * 4.0),
            64.0,
            "8.9",
            None,
        ));
    }
    let load_balancer = LoadBalancer::with_runtime_profile(Arc::clone(&cluster3), &runtime_profile);
    let backend = ExecutionBackend::from_runtime_profile(&runtime_profile);
    let input: Vec<f32> = (0..8192).map(|index| index as f32 * 0.5).collect();
    bench("autotune: detect_runtime_profile_fast", 20_000, || {
        let _ = detect_runtime_profile("bench-local");
    });
    bench("autotune: detect_runtime_profile_full", 5_000, || {
        let _ = detect_runtime_profile_with_mode("bench-local", ProbeMode::Full);
    });
    bench("autotune: load_balance 80 layers", 100_000, || {
        let _ = load_balancer.distribute_layers_with_runtime_profile(&layers_80, &runtime_profile);
    });
    bench("autotune: accelerator scale_f32_slice", 20_000, || {
        let _ = backend.scale_f32_slice(&input, 1.5);
    });

    println!("{}", "-".repeat(82));
    println!(
        "\nPlatform: {} | Profile: release (optimized)\n",
        std::env::consts::ARCH
    );
}
