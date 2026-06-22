use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion};
use ghostlink_core::{
    cluster::ClusterState,
    planning::{assign_layers_sequentially, LayerSpec},
    protocol::{DiscoveryFrame, FrameKind, NodeResources},
    ring::{RingConfig, SpscRingBuffer},
};

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

    let mut group = c.benchmark_group("planning");
    group.bench_function("33_layers_2_nodes", |b| {
        b.iter(|| black_box(assign_layers_sequentially(black_box(&nodes_2), black_box(&layers_33))));
    });
    group.bench_function("80_layers_8_nodes", |b| {
        b.iter(|| black_box(assign_layers_sequentially(black_box(&nodes_8), black_box(&layers_80))));
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

fn criterion_benches(c: &mut Criterion) {
    bench_ring(c);
    bench_protocol(c);
    bench_planning(c);
    bench_cluster(c);
}

criterion_group!(benches, criterion_benches);
criterion_main!(benches);
