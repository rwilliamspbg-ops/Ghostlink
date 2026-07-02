/// Tensor streaming fabric benchmark for multi-node inference
/// Simplified test harness measuring throughput and latency only

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use ghostlink_core::runtime::{
    execute_pipeline, execute_pipeline_tcp_loopback, PipelinePlan, StagePlacement, DeviceKind,
};

/// Baseline: zero-copy in-memory execution (single GPU)
fn benchmark_inmem_baseline(c: &mut Criterion) {
    let plan = PipelinePlan {
        stages: vec![
            StagePlacement {
                node_id: "gpu-0".to_string(),
                start_layer: 0,
                end_layer: 30,
                device: DeviceKind::Gpu,
                est_latency_ms: 1.5,
            },
        ],
    };

    c.bench_function("fabric_inmem_single_gpu", |b| {
        b.iter(|| {
            let result = execute_pipeline(&plan, black_box(128), 4);
            (result.throughput_tokens_per_sec, result.avg_token_latency_ms)
        });
    });
}

/// TCP loopback: two-stage split with transport overhead
fn benchmark_tcp_two_stage_split(c: &mut Criterion) {
    let plan = PipelinePlan {
        stages: vec![
            StagePlacement {
                node_id: "stage-0".to_string(),
                start_layer: 0,
                end_layer: 16,
                device: DeviceKind::Gpu,
                est_latency_ms: 0.75,
            },
            StagePlacement {
                node_id: "stage-1".to_string(),
                start_layer: 16,
                end_layer: 32,
                device: DeviceKind::Gpu,
                est_latency_ms: 0.75,
            },
        ],
    };

    c.bench_function("fabric_tcp_two_stage_split", |b| {
        b.iter(|| {
            let result = execute_pipeline_tcp_loopback(&plan, black_box(128), 4)
                .expect("loopback failed");
            (result.throughput_tokens_per_sec, result.avg_token_latency_ms)
        });
    });
}

/// TCP loopback: four-stage split (deeper serial bottleneck)
fn benchmark_tcp_four_stage_split(c: &mut Criterion) {
    let plan = PipelinePlan {
        stages: vec![
            StagePlacement {
                node_id: "stage-0".to_string(),
                start_layer: 0,
                end_layer: 8,
                device: DeviceKind::Gpu,
                est_latency_ms: 0.4,
            },
            StagePlacement {
                node_id: "stage-1".to_string(),
                start_layer: 8,
                end_layer: 16,
                device: DeviceKind::Cpu,
                est_latency_ms: 0.4,
            },
            StagePlacement {
                node_id: "stage-2".to_string(),
                start_layer: 16,
                end_layer: 24,
                device: DeviceKind::Gpu,
                est_latency_ms: 0.4,
            },
            StagePlacement {
                node_id: "stage-3".to_string(),
                start_layer: 24,
                end_layer: 32,
                device: DeviceKind::Cpu,
                est_latency_ms: 0.4,
            },
        ],
    };

    c.bench_function("fabric_tcp_four_stage_split", |b| {
        b.iter(|| {
            let result = execute_pipeline_tcp_loopback(&plan, black_box(128), 4)
                .expect("loopback failed");
            (result.throughput_tokens_per_sec, result.avg_token_latency_ms)
        });
    });
}

/// Micro-batch size effect on latency-per-token
fn benchmark_micro_batch_latency(c: &mut Criterion) {
    let plan = PipelinePlan {
        stages: vec![
            StagePlacement {
                node_id: "prod".to_string(),
                start_layer: 0,
                end_layer: 16,
                device: DeviceKind::Gpu,
                est_latency_ms: 0.5,
            },
            StagePlacement {
                node_id: "cons".to_string(),
                start_layer: 16,
                end_layer: 32,
                device: DeviceKind::Gpu,
                est_latency_ms: 0.5,
            },
        ],
    };

    c.bench_function("fabric_tcp_micro_batch_latency", |b| {
        b.iter(|| {
            let result = execute_pipeline_tcp_loopback(&plan, black_box(128), black_box(4))
                .expect("latency bench failed");
            (result.avg_token_latency_ms, result.p95_token_latency_ms)
        });
    });
}

criterion_group!(
    name = benches;
    config = Criterion::default()
        .sample_size(10)
        .significance_level(0.1)
        .measurement_time(std::time::Duration::from_secs(5));
    targets =
        benchmark_inmem_baseline,
        benchmark_tcp_two_stage_split,
        benchmark_tcp_four_stage_split,
        benchmark_micro_batch_latency,
);

criterion_main!(benches);
