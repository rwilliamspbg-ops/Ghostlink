use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion};
use ghostlink_core::{
    accelerator::ExecutionBackend,
    cluster::ClusterState,
    host::{
        detect_runtime_profile, detect_runtime_profile_with_mode, AccelerationMode, ProbeMode,
        RuntimeProfile,
    },
    load_balance::LoadBalancer,
    planning::{assign_layers_sequentially, assign_layers_with_runtime_profile, LayerSpec},
    protocol::{DiscoveryFrame, FrameKind, NodeResources},
    ring::{RingConfig, SpscRingBuffer},
};
use std::sync::Arc;

fn bench_ring(c: &mut Criterion) {
    let ring = SpscRingBuffer::<u64>::new(RingConfig::default());
    let mut group = c.benchmark_group("ring");

    group.bench_function(BenchmarkId::new("push_pop_round_trip", "st"), |b| {
        let mut counter = 0u64;
        b.iter(|| {
            black_box(ring.push(black_box(counter))).ok();
            black_box(ring.pop());
            counter = counter.wrapping_add(1);
        });
    });

    group.bench_function(BenchmarkId::new("push_only", "st"), |b| {
        b.iter(|| {
            if black_box(ring.push(black_box(42u64))).is_err() {
                black_box(ring.pop());
            }
        });
    });

    group.bench_function(BenchmarkId::new("spsc_throughput", "mt"), |b| {
        b.iter_custom(|iters| {
            let ring = Arc::new(SpscRingBuffer::<u64>::new(RingConfig::default()));
            let producer_ring = Arc::clone(&ring);
            let consumer_ring = Arc::clone(&ring);

            let start = std::time::Instant::now();
            let producer = std::thread::spawn(move || {
                for i in 0..iters {
                    while producer_ring.push(i).is_err() {
                        core::hint::spin_loop();
                    }
                }
            });

            for _ in 0..iters {
                while consumer_ring.pop().is_none() {
                    core::hint::spin_loop();
                }
            }

            producer.join().unwrap();
            start.elapsed()
        });
    });

    group.finish();
}

fn bench_protocol(c: &mut Criterion) {
    let node = NodeResources::new("bench-node", 24.0, 64.0, "8.9", None);
    let frame = DiscoveryFrame {
        kind: FrameKind::Discovery,
        node,
    };
    let encoded = frame.encode();

    let mut group = c.benchmark_group("protocol");
    group.bench_function("encode", |b| {
        b.iter(|| black_box(frame.encode()));
    });
    group.bench_function("decode", |b| {
        b.iter(|| {
            let _ = black_box(DiscoveryFrame::decode(black_box(&encoded)));
        });
    });
    group.bench_function("round_trip", |b| {
        b.iter(|| {
            let encoded = black_box(frame.encode());
            let _ = black_box(DiscoveryFrame::decode(&encoded));
        });
    });
    group.finish();
}

fn bench_planning(c: &mut Criterion) {
    let nodes_2: Vec<_> = (0..2)
        .map(|i| NodeResources::new(format!("node-{i}"), 24.0, 64.0, "8.9", None))
        .collect();
    let layers_33: Vec<_> = (0..33)
        .map(|i| LayerSpec {
            index: i,
            vram_gb: 1.0,
            num_weights: 0,
        })
        .collect();

    let nodes_8: Vec<_> = (0..8)
        .map(|i| NodeResources::new(format!("node-{i}"), 48.0, 128.0, "9.0", None))
        .collect();
    let layers_80: Vec<_> = (0..80)
        .map(|i| LayerSpec {
            index: i,
            vram_gb: 1.0,
            num_weights: 0,
        })
        .collect();
    let runtime_profile = RuntimeProfile {
        node_resources: NodeResources::new("bench-host", 24.0, 64.0, "8.9", None),
        logical_cores: 16,
        recommended_workers: 8,
        acceleration_mode: AccelerationMode::Gpu,
        xdp_supported: true,
        detection_source: String::from("bench"),
        probe_mode: ghostlink_core::ProbeMode::Fast,
    };

    let mut group = c.benchmark_group("planning");
    group.bench_function("33_layers_2_nodes", |b| {
        b.iter(|| {
            black_box(assign_layers_sequentially(
                black_box(&nodes_2),
                black_box(&layers_33),
            ))
        });
    });
    group.bench_function("80_layers_8_nodes", |b| {
        b.iter(|| {
            black_box(assign_layers_sequentially(
                black_box(&nodes_8),
                black_box(&layers_80),
            ))
        });
    });
    group.bench_function("80_layers_8_nodes_autotuned", |b| {
        b.iter(|| {
            black_box(assign_layers_with_runtime_profile(
                black_box(&nodes_8),
                black_box(&layers_80),
                black_box(&runtime_profile),
            ))
        });
    });
    group.finish();
}

fn bench_cluster(c: &mut Criterion) {
    let cluster = ClusterState::new();
    let snapshot_cluster = ClusterState::new();
    for i in 0..10 {
        snapshot_cluster.register(NodeResources::new(
            format!("node-{i}"),
            24.0,
            64.0,
            "8.9",
            None,
        ));
    }

    let mut group = c.benchmark_group("cluster");
    group.bench_function("register_update", |b| {
        b.iter_batched(
            || NodeResources::new("bench", 24.0, 64.0, "8.9", None),
            |node| cluster.register(node),
            BatchSize::SmallInput,
        );
    });
    group.bench_function("nodes_snapshot_10", |b| {
        b.iter(|| black_box(snapshot_cluster.nodes_snapshot()));
    });
    group.bench_function("total_vram_10", |b| {
        b.iter(|| black_box(snapshot_cluster.total_vram_gb()));
    });
    group.finish();
}

fn bench_autotune(c: &mut Criterion) {
    let cluster = Arc::new(ClusterState::new());
    for i in 0..8 {
        cluster.register(NodeResources::new(
            format!("node-{i}"),
            24.0 + (i as f32 * 4.0),
            64.0,
            "8.9",
            None,
        ));
    }
    let layers: Vec<_> = (0..80)
        .map(|i| LayerSpec {
            index: i,
            vram_gb: 1.0,
            num_weights: 0,
        })
        .collect();
    let runtime_profile = RuntimeProfile {
        node_resources: NodeResources::new("bench-host", 24.0, 64.0, "8.9", None),
        logical_cores: 16,
        recommended_workers: 8,
        acceleration_mode: AccelerationMode::Gpu,
        xdp_supported: true,
        detection_source: String::from("bench"),
        probe_mode: ghostlink_core::ProbeMode::Fast,
    };
    let load_balancer = LoadBalancer::with_runtime_profile(Arc::clone(&cluster), &runtime_profile);
    let backend = ExecutionBackend::from_runtime_profile(&runtime_profile);
    let input: Vec<f32> = (0..8192).map(|index| index as f32 * 0.5).collect();

    let mut group = c.benchmark_group("autotune");
    group.bench_function("detect_runtime_profile_fast", |b| {
        b.iter(|| black_box(detect_runtime_profile("bench-local")));
    });
    group.bench_function("detect_runtime_profile_full", |b| {
        b.iter(|| {
            black_box(detect_runtime_profile_with_mode(
                "bench-local",
                ProbeMode::Full,
            ))
        });
    });
    group.bench_function("load_balance_80_layers_autotuned", |b| {
        b.iter(|| {
            black_box(load_balancer.distribute_layers_with_runtime_profile(
                black_box(&layers),
                black_box(&runtime_profile),
            ))
        });
    });
    group.bench_function("accelerator_scale_f32_slice", |b| {
        b.iter(|| black_box(backend.scale_f32_slice(black_box(&input), black_box(1.5))));
    });
    group.finish();
}

fn criterion_benches(c: &mut Criterion) {
    bench_ring(c);
    bench_protocol(c);
    bench_planning(c);
    bench_cluster(c);
    bench_autotune(c);
}

criterion_group!(benches, criterion_benches);
criterion_main!(benches);
