# Ghostlink Studio - Performance Benchmarks

## Overview

This document contains performance benchmarks for the Ghostlink Studio system, measuring throughput and latency across different configurations.

**Note**: All benchmarks are measured with optimized in-memory transport. AF_XDP kernel-bypass is not currently implemented; see `README.md` for details.

---

## Full-Spectrum Session Benchmark — 2026-07-25 (laptop iGPU host)

Real, measured results from a single benchmarking session on the hardware below,
following the repo's own validated command set (`TESTING.md`). Reported here
because this host's profile (modest integrated GPU, Windows, native dev
launch) differs sharply from the RTX 4090 desktop numbers earlier in this
document — both are legitimate, they're just different machines.

### Hardware

| | |
|---|---|
| CPU | 16 logical cores |
| System RAM | 27.6 GB |
| GPU | AMD Radeon(TM) 860M (integrated) |
| GPU backend | DirectML, 4.0 GB VRAM |
| OS | Windows 11 |
| Build | `cargo build --release` — workspace `lto = "thin"`, `codegen-units = 1` (already the repo default) |

Detected via `ghost-link.exe probe local-node` — fast and full probe modes
agreed, so hardware detection needed no fixes this round.

### Methodology note: this host is noisy

Two back-to-back `cargo bench -p ghostlink-core --bench criterion` runs, **zero
code changes between them**, showed 5-80% swings on most benchmarks and a
one-off 3-4x hit on the multi-threaded ring buffer test. A 12-round
interleaved A/B (below) put steady-state flow throughput stdev at ~15-19% of
the mean. Treat single-run deltas on this machine with real skepticism —
absolute numbers and multi-run distributions are the trustworthy signal, not
one-shot "improved/regressed" percentages.

### Primitives (Criterion, `cargo bench -p ghostlink-core --bench criterion`)

Absolute timings from a clean run (background services stopped):

| Benchmark | Time |
|---|---|
| `ring/push_pop_round_trip/st` | 1.13 ns |
| `ring/push_only/st` | 2.6-2.7 ns |
| `ring/spsc_throughput/mt` | 7-11 ns (noisiest primitive — thread-scheduling sensitive) |
| `protocol/encode` | 61.2-61.8 ns |
| `protocol/decode` | 90-95 ns |
| `protocol/round_trip` | 154-224 ns |
| `planning/33_layers_2_nodes` | 89-112 ns |
| `planning/80_layers_8_nodes` | 117-140 ns |
| `planning/80_layers_8_nodes_autotuned` | 283-292 ns |
| `cluster/register_update` | 161-183 ns |
| `cluster/nodes_snapshot_10` | 18-20 ns |
| `cluster/calculate_cluster_health_10` | 15-16.5 ns |
| `autotune/detect_runtime_profile_fast` | 149-256 ns |
| `autotune/detect_runtime_profile_full` | 1.38 s (real hardware/WMI probe — startup-only cost, not a hot path) |
| `autotune/load_balance_80_layers_autotuned` | 1.23-1.31 µs |
| `autotune/accelerator_scale_f32_slice` | 577-641 ns |
| `fabric_inmem_single_gpu` | 322 µs |
| `fabric_tcp_two_stage_split` | 909 µs |
| `fabric_tcp_micro_batch_latency` | 1.16 ms |
| `fabric_tcp_four_stage_split` | 70.5 ms |

Full artifact: `python3 scripts/summarize_criterion_report.py --criterion-root target/criterion --output artifacts/criterion-summary.json`.

### Full pipeline (`scripts/flow_perf_snapshot.py`, release build)

`docs/PERF_BASELINE.json` and `docs/PERF_BASELINE_STRESS.json` were both
captured at `exec_tokens=512 micro_batch=8` — matching that (rather than the
script's own bare-invocation default of 256/4, which compares a different
workload) is required for a meaningful drift check:

| Mode | Throughput (avg) | P95 | vs. baseline |
|---|---|---|---|
| tcp (matched to `PERF_BASELINE.json`) | 228,626 tok/s | 2.20 ms | -10.7% (baseline 256,020) — within 45% drop tolerance |
| inmem (matched to `PERF_BASELINE.json`) | 346,932 tok/s | 1.39 ms | -31.5% (baseline 506,809) — within 40% drop tolerance |
| tcp (stress profile, 12 runs) | 234,983 tok/s | 2.14 ms | passed vs. `PERF_BASELINE_STRESS.json` |
| inmem (stress profile, 12 runs) | 403,635 tok/s | 1.21 ms | passed vs. `PERF_BASELINE_STRESS.json` |

`check_perf_drift.py`, `validate_stage_tail_metrics.py`, `validate_flow_canary.py`,
and `validate_flow_metrics_schema_contract.py` all passed on this run. Net
read: this laptop iGPU is meaningfully slower than whichever machine set the
baseline, but well inside the repo's drift tolerances — no regression.

Per-stage percentile analysis (`analyze_flow_stage_metrics.py`) showed
`avg_recv_wait_ms` growing ~20-25x from stage 0 (0.0008 ms) to stage 8-10
(0.017-0.02 ms) — expected pipeline-depth backpressure accumulation across 11
sequential stages, not a bug, but relevant if tuning stage count for
lower-latency topologies.

### TCP autotune

`GHOSTLINK_TCP_AUTOTUNE=1` selected `max_inflight=64` over the default 256.
An initial 2-sample head-to-head suggested the default was ~20-30% faster; a
proper 12-round **interleaved** A/B (to control for time-based drift)
contradicted that:

| Config | Mean | Stdev |
|---|---|---|
| `max_inflight=64` | 238,393 tok/s | 36,629 (15%) |
| `max_inflight=256` | 252,035 tok/s | 47,642 (19%) |

The two distributions overlap almost completely — not a statistically
reliable difference on this hardware. Lesson for future runs on this class of
machine: don't trust a 2-3 sample comparison here, and note that
`GHOSTLINK_TCP_AUTOTUNE_TOKENS` defaults to 64 tokens regardless of your real
`execution_tokens`, so pass `GHOSTLINK_TCP_AUTOTUNE_TOKENS`/`_MICRO_BATCH`
matching your target workload if you want the sweep to optimize for it.

### llama-server flags (`docs/LOCAL_INFERENCE_TUNING.md`)

Tested default vs. this host's documented 4GB-VRAM-tier recommendation
(`-fa on -b 512 -ub 256 -ctk q8_0 -ctv q8_0`) on `Llama-3.2-1B-Instruct-IQ3_M`:

| Metric | Default | Tuned | Delta |
|---|---|---|---|
| Decode (200 tok, short prompt) | 75.0 tok/s avg | 75.0 tok/s avg | ~0% (expected — decode of a single stream doesn't benefit from batch/flash-attn tuning) |
| Prefill (1381-token prompt, cold) | 1,043.5 tok/s | 1,169.7 tok/s | +12% (expected direction — matches `LOCAL_INFERENCE_TUNING.md`) |

### What wasn't run

- **Flamegraph**: `cargo-flamegraph` 0.6.13 is installed, but Windows profiling
  goes through `blondie` (ETW), which requires an elevated terminal —
  `NotAnAdmin` from this session's shell. Also, the `flow` command itself
  completes in ~1-2 ms per invocation, too short for meaningful sampling
  without wrapping it in a loop or profiling the Criterion binary instead
  (which runs long enough). To profile later: open an Administrator terminal
  and run `flamegraph -o out.svg -- target\release\deps\criterion-<hash>.exe --bench`.
- AF_XDP kernel-bypass — not implemented on Windows, out of scope here.


## In-Memory Transport Benchmarks

### CPU Configuration (8-core Intel/AMD)

| Model | Throughput | Latency P50 | Latency P95 | Memory Usage |
|-------|------------|-------------|-------------|--------------|
| orca-mini (3B) | ~600K tokens/s | 1.2ms | 3.5ms | 2.0 GB |
| mistral (7B) | ~450K tokens/s | 1.8ms | 5.2ms | 6.0 GB |
| llama2-7b (7B) | ~420K tokens/s | 2.0ms | 5.8ms | 6.0 GB |

### GPU Configuration (NVIDIA RTX 4090)

| Model | Throughput | Latency P50 | Latency P95 | Memory Usage |
|-------|------------|-------------|-------------|--------------|
| orca-mini (3B) | ~2.1M tokens/s | 0.4ms | 1.1ms | 2.0 GB |
| mistral (7B) | ~1.6M tokens/s | 0.6ms | 1.8ms | 6.0 GB |
| llama2-7b (7B) | ~1.5M tokens/s | 0.7ms | 2.0ms | 6.0 GB |

---

## Multi-Node Performance

### LAN Performance (1 Gbps Ethernet)

| Node Count | Throughput | Latency | Notes |
|------------|------------|---------|-------|
| 2 nodes | ~580K tokens/s | 2.5ms | TCP transport |
| 3 nodes | ~550K tokens/s | 3.2ms | TCP transport |
| 4 nodes | ~520K tokens/s | 4.1ms | TCP transport |

### Notes on Multi-Node Performance

- Throughput decreases slightly with more nodes due to network overhead
- Latency increases linearly with node count
- Network bandwidth is the primary bottleneck for multi-node setups

---

## Memory Requirements

### Model Memory Footprint

| Model | Parameters | Size (GGUF) | VRAM Required | System RAM Recommended |
|-------|-----------|-------------|---------------|------------------------|
| orca-mini | 3B | 1.7 GB | 2.0 GB | 8 GB |
| phi | 3B | 2.0 GB | 3.0 GB | 8 GB |
| mistral | 7B | 4.1 GB | 6.0 GB | 16 GB |
| neural-chat | 7B | 4.0 GB | 6.0 GB | 16 GB |
| llama2-7b | 7B | 3.8 GB | 5.5 GB | 16 GB |
| openhermes | 7B | 4.1 GB | 6.0 GB | 16 GB |
| llama2-13b | 13B | 7.3 GB | 10.0 GB | 32 GB |
| mistral-medium | 13B | 8.0 GB | 12.0 GB | 32 GB |
| codeup | 13B | 7.5 GB | 10.0 GB | 32 GB |
| dolphin-mixtral | 8x7B | 26.0 GB | 32.0 GB | 64 GB |
| llama2-70b | 70B | 39.0 GB | 48.0 GB | 128 GB |

---

## Benchmark Methodology

### Test Configuration

- **Hardware**: Intel Core i9-13900K (24 cores), NVIDIA RTX 4090 (24GB VRAM)
- **RAM**: 64GB DDR5-5600
- **Storage**: NVMe SSD (PCIe 4.0 x4)
- **Network**: 1 Gbps Ethernet

### Test Procedure

1. Load model into memory
2. Run inference on synthetic prompts (10K tokens)
3. Measure throughput and latency
4. Repeat 3 times and average results

### Metrics

- **Throughput**: Tokens generated per second
- **Latency P50**: 50th percentile latency (median)
- **Latency P95**: 95th percentile latency (includes outliers)
- **Memory Usage**: Peak VRAM and system RAM usage

---

## Performance Tips

### Maximizing Throughput

1. **Use GPU when available**: GPU can provide 3-4x speedup over CPU
2. **Batch requests**: Larger batches improve throughput
3. **Use quantized models**: GGUF q4_0 or q5_0 quantization maintains quality while reducing memory

### Reducing Latency

1. **Keep model loaded**: Avoid loading/unloading frequently
2. **Use smaller models**: 3B-7B models have lower latency
3. **Enable GPU offloading**: Move layers to GPU when possible

### Multi-Node Optimization

1. **Use fast network**: 10 Gbps recommended for multi-node
2. **Minimize nodes**: More nodes = more network overhead
3. **Balance load**: Distribute layers evenly across nodes

---

## Future Improvements

### Planned Optimizations

- [ ] AF_XDP kernel-bypass implementation (requires Linux kernel 5.17+)
- [ ] Quantization-aware scheduling
- [ ] Layer parallelism optimization
- [ ] Continuous batching for variable-length inputs

### Known Limitations

- In-memory transport is currently the only implemented option
- AF_XDP requires Linux kernel 5.17+ and eBPF tooling
- Multi-node performance limited by network bandwidth

---

## See Also

- [README.md](../README.md) - Project overview and installation
- [IMPLEMENTATION_GUIDE.md](../IMPLEMENTATION_GUIDE.md) - Code structure
- [GHOSTLINK_FIX_PLAN.md](../GHOSTLINK_FIX_PLAN.md) - Remediation plan
