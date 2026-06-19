/// Performance Baseline for Ghost-Link Primitives

use std::time::Instant;
use std::sync::Arc;
use std::thread;

use ghostlink_core::{
    ring::{SpscRingBuffer, RingConfig},
    protocol::{DiscoveryFrame, FrameKind, NodeResources},
    cluster::ClusterState,
    planning::{assign_layers_sequentially, LayerSpec},
};

fn bench(name: &str, iters: u64, mut f: impl FnMut()) -> f64 {
    for _ in 0..1000_u64.min(iters / 10) { f(); }
    let start = Instant::now();
    for _ in 0..iters { f(); }
    let elapsed = start.elapsed();
    let ns = elapsed.as_nanos() as f64 / iters as f64;
    let ops = 1_000_000_000.0 / ns;
    println!("{:<52} {:>10.2} ns/op  {:>14.0} ops/sec", name, ns, ops);
    ns
}

fn main() {
    println!("\nGhost-Link Performance Baseline");
    println!("================================");
    println!("{:<52} {:>10}       {:>14}", "Benchmark", "Latency", "Throughput");
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
            if ring.push(42u64).is_err() { ring.pop(); }
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
                    if prod.push(i).is_ok() { break; }
                    thread::yield_now();
                }
            }
        });
        let consumer = thread::spawn(move || {
            let mut n = 0u64;
            while n < iters {
                if cons.pop().is_some() { n += 1; } else { thread::yield_now(); }
            }
        });
        producer.join().unwrap();
        consumer.join().unwrap();
        let elapsed = start.elapsed();
        let ns = elapsed.as_nanos() as f64 / iters as f64;
        let ops = 1_000_000_000.0 / ns;
        println!("{:<52} {:>10.2} ns/op  {:>14.0} ops/sec",
            "ring_buffer: SPSC cross-thread (10k items)", ns, ops);
    }

    // ─── Protocol ────────────────────────────────────────────────────────────
    let node = NodeResources::new("bench-node", 24.0, 64.0, "8.9", None);
    let frame = DiscoveryFrame { kind: FrameKind::Discovery, node };

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
    let nodes_2: Vec<_> = (0..2).map(|i| {
        NodeResources::new(format!("node-{}", i), 24.0, 64.0, "8.9", None)
    }).collect();
    let layers_33: Vec<LayerSpec> = (0..33).map(|i| LayerSpec {
        index: i, vram_gb: 1.0, num_weights: 0
    }).collect();
    bench("planning: 33 layers across 2 nodes", 100_000, || {
        let _ = assign_layers_sequentially(&nodes_2, &layers_33);
    });

    let nodes_8: Vec<_> = (0..8).map(|i| {
        NodeResources::new(format!("node-{}", i), 48.0, 128.0, "9.0", None)
    }).collect();
    let layers_80: Vec<LayerSpec> = (0..80).map(|i| LayerSpec {
        index: i, vram_gb: 1.0, num_weights: 0
    }).collect();
    bench("planning: 80 layers across 8 nodes", 100_000, || {
        let _ = assign_layers_sequentially(&nodes_8, &layers_80);
    });

    // ─── Cluster State ───────────────────────────────────────────────────────
    let cluster = ClusterState::new();
    bench("cluster: register node (update path)", 100_000, || {
        cluster.register(NodeResources::new("bench", 24.0, 64.0, "8.9", None));
    });

    let cluster2 = ClusterState::new();
    for i in 0..10 {
        cluster2.register(NodeResources::new(
            format!("node-{}", i), 24.0, 64.0, "8.9", None));
    }
    bench("cluster: nodes() snapshot (10 nodes)", 200_000, || {
        let _ = cluster2.nodes();
    });
    bench("cluster: total_vram_gb() (10 nodes)", 500_000, || {
        let _ = cluster2.total_vram_gb();
    });

    println!("{}", "-".repeat(82));
    println!("\nPlatform: {} | Profile: release (optimized)\n",
        std::env::consts::ARCH);
}
