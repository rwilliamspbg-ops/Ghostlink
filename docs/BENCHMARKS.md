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

### Cross-model chat throughput — 2026-08-03 follow-up, same host

Real generation numbers across three locally-downloaded models, prompted by a
live report of ~2-minute chat timeouts on `Qwen3.5-9B-Q4_K_M`. Same prompt
("List ten uses for a smartphone, one per line.", 11-token prompt), same
`n_predict=150`, one discarded warm-up completion per model first (avoids
first-call Vulkan shader-compile skew) — single run per model, not the
multi-round averages the rest of this doc uses, so treat deltas here with the
same skepticism the noisy-host section above asks for.

| Model | Size on disk | Quant | Load time | Prefill | Decode |
|---|---|---|---|---|---|
| `Qwen3.5-9B-Q4_K_M` | 5.6 GB | Q4_K_M | 14.1 s | ~6.0 tok/s | ~11.2 tok/s |
| `Meta-Llama-3-8B-Instruct-IQ3_M` | 3.8 GB | IQ3_M | 9.9 s | 43.4 tok/s | 12.8 tok/s |
| `Llama-3.2-1B-Instruct-IQ3_M` | 657 MB | IQ3_M | 4.3 s | 375.3 tok/s | 74.8 tok/s |

Confirmed this is genuine GPU work, not a silent CPU fallback:
`Get-Counter '\GPU Engine(*)\Utilization Percentage'` showed the
`llama-server` process's `engtype_compute` instance at ~29% during the
`Qwen3.5-9B` generation.

Caveat: this compares model *and* architecture together (Qwen3.5 vs Llama-3),
not quantization in isolation — no Q3_K build of `Qwen3.5-9B` itself was on
disk to test, so it's not a controlled same-model comparison. Still, at a
similar parameter count (8B vs 9B) the more-compressed IQ3_M build's 7x
prefill advantage over Q4_K_M is a large enough gap that quantization is
clearly doing real work here, on top of whatever the architecture difference
contributes.

### What wasn't run

- **Flamegraph**: `cargo-flamegraph` 0.6.13 is installed, but Windows profiling
  goes through `blondie` (ETW), which requires an elevated terminal —
  `NotAnAdmin` from this session's shell. Also, the `flow` command itself
  completes in ~1-2 ms per invocation, too short for meaningful sampling
  without wrapping it in a loop or profiling the Criterion binary instead
  (which runs long enough). To profile later: open an Administrator terminal
  and run `flamegraph -o out.svg -- target\release\deps\criterion-<hash>.exe --bench`.
- AF_XDP kernel-bypass — not implemented on Windows, out of scope here.


## Full-Spectrum Session Benchmark — 2026-08-03 (Linux mini PC, no dedicated GPU)

Real, measured results from a genuinely different hardware class than
every other entry in this document — a low-power consumer mini PC with no
dedicated GPU, running native Linux rather than Windows. This is the real
Linux data point [ENTERPRISE_PLAN.md](ENTERPRISE_PLAN.md)'s Track A flagged
as missing (this is also the same host used for the real two-machine LAN
benchmark in the Multi-Node Performance section below).

### Hardware

| | |
|---|---|
| Host | `Desk-Mini` |
| CPU | 4 logical cores (Intel Alder Lake-N) |
| System RAM | 14.9 GB |
| GPU | Alder Lake-N UHD Graphics (integrated), Vulkan, 0.0 GB dedicated VRAM reported |
| OS | Linux |
| Build | `cargo build --release` (workspace default `lto = "thin"`, `codegen-units = 1`) |

Detected via `ghost-link probe local-node` (same command used for the
Multi-Node Performance entry below).

### Methodology note: contention explains the primitives, but not the full pipeline

The first `cargo bench`/`flow_perf_snapshot.py` run on this host ran while
`launch.sh`'s own servers (API, control-plane, llama-server, Vite dev
server) were still up, competing for all 4 cores. That run showed Criterion
reporting "Performance has regressed +20-40%" on nearly every primitive
relative to this host's previously-stored baseline — a real, expected
effect of CPU contention. After stopping every Ghostlink process and
re-running clean, most primitives improved as expected (raw logs below).

The full-pipeline `flow_perf_snapshot.py` numbers did **not** follow that
pattern — the "clean" run's average throughput was *lower* than the
contended run's (tcp: 76.5k clean vs. 114.4k contended tok/s; inmem: 134.4k
clean vs. 178.2k contended tok/s), and both runs show an enormous
run-to-run spread within their own 5 samples (clean inmem alone:
77.5k-187.4k tok/s, a 2.4x range). On this specific low-power 4-core host,
5 runs isn't enough for a stable mean, and other confounds (thermal
throttling on an N-series chip, memory-bandwidth sharing with the
integrated GPU) likely dominate over whatever contention stopping the
app's own servers removed. Treat every number below as directional for "a
weak consumer-grade mini PC," not a tight estimate — this host is noisier
than the AMD laptop profile above, which was already flagged as noisy.

### Primitives (Criterion, clean run — background services stopped)

| Benchmark | Time |
|---|---|
| `ring/push_pop_round_trip/st` | 3.15-3.22 ns |
| `ring/push_only/st` | 5.07-5.15 ns |
| `ring/spsc_throughput/mt` | 12.15-12.83 ns |
| `protocol/encode` | 85.1-86.3 ns |
| `protocol/decode` | 101.4-104.0 ns |
| `protocol/round_trip` | 204.0-212.6 ns |
| `planning/33_layers_2_nodes` | 140.1-141.0 ns |
| `planning/80_layers_8_nodes` | 334.2-342.4 ns |
| `planning/80_layers_8_nodes_autotuned` | 424.6-428.6 ns |
| `cluster/register_update` | 235.3-237.3 ns |
| `cluster/nodes_snapshot_10` | 30.8-31.4 ns |
| `cluster/total_vram_10` | 927-935 ps |
| `cluster/calculate_cluster_health_10` | 45.1-47.0 ns |
| `autotune/detect_runtime_profile_fast` | 106.9-108.8 ns |
| `autotune/detect_runtime_profile_full` | 19.18-20.48 ms |
| `autotune/load_balance_80_layers_autotuned` | 1.616-1.632 µs |
| `autotune/accelerator_scale_f32_slice` | 2.186-2.224 µs |

`fabric_*` benchmarks (present in the Windows laptop entry above) did not
appear in this run's output on this host — noted rather than guessed at;
worth checking whether that's a platform feature gate or a suite change
before assuming anything from their absence.

### Full pipeline (`scripts/flow_perf_snapshot.py`, `exec_tokens=512 micro_batch=8`, release build)

| Run | Mode | Throughput (avg) | Throughput (min-max) | P95 (avg) |
|---|---|---|---|---|
| Contended (app servers running) | tcp | 114,429.68 tok/s | 78,979.10-134,124.42 | 4.58 ms |
| Contended (app servers running) | inmem | 178,194.78 tok/s | 82,469.41-272,413.03 | 3.56 ms |
| Clean (servers stopped) | tcp | 76,455.51 tok/s | 63,534.12-89,957.73 | 6.78 ms |
| Clean (servers stopped) | inmem | 134,359.68 tok/s | 77,473.96-187,417.62 | 4.10 ms |

Compared against `docs/PERF_BASELINE.json` (256,020 tok/s tcp / 506,809
tok/s inmem, captured on different, more powerful hardware): this host's
clean-run numbers land roughly 70-73% below that baseline — far outside the
45%/40% drop tolerances defined there. That's not a regression; it's
confirmation that a 4-core integrated-graphics mini PC is a genuinely
different performance tier from whatever machine set that baseline. The
honest takeaway from this host isn't a specific throughput figure (the
spread above is too wide to trust any single number) — it's that Ghostlink
runs correctly end-to-end on real, low-power, no-dedicated-GPU Linux
hardware, which is itself a useful data point for the project's "runs on
whatever's already on your LAN" positioning.

Raw logs (on the Desk-Mini host itself, not committed to this repo):
`~/criterion_desk_mini.log` / `~/flow_perf_desk_mini.log` (contended run)
and `~/criterion_desk_mini_clean.log` / `~/flow_perf_desk_mini_clean.log`
(clean run).

## Other hardware profiles

Real, measured single-node numbers now exist in this document for three
hardware classes: an AMD integrated GPU laptop (Windows), an Intel
i7-14700K (Linux/WSL2, see the README's own
[Performance section](../README.md#performance)), and a low-power Linux
mini PC with no dedicated GPU (above). This repo previously carried
fabricated placeholder tables here for CPU/GPU configurations (8-core
Intel/AMD, RTX 4090) and model throughput that were never actually run —
they've been removed rather than left as bait for anyone skimming this
doc. Still missing: a discrete NVIDIA/AMD GPU, Apple Silicon, and a
server-class CPU. If you benchmark Ghostlink on one of those,
`scripts/flow_perf_snapshot.py` and
`cargo bench -p ghostlink-core --bench criterion` are the repo's real,
runnable benchmark tools (see the Full-Spectrum sessions above for exact
invocations and output format) — a PR adding a dated, methodology-labeled
entry for a new hardware class is a genuinely useful contribution.

---

## Multi-Node Performance

### Real two-machine LAN run — 2026-08-03

The first real entry for this section, replacing the fabricated 2/3/4-node
placeholder table this repo used to carry (see git history) — that table
predated `stage-worker`/`flow --remote-addr` even existing, so no version
of Ghost-Link could have produced it.

**Hardware — two genuinely separate machines on the same residential LAN:**

| | Coordinator (local) | Remote (`stage-worker`) |
|---|---|---|
| Host | Windows 11 laptop | `Desk-Mini` (Linux) |
| CPU | 16 logical cores | 4 logical cores (Intel Alder Lake-N) |
| System RAM | 27.6 GB | 14.9 GB |
| GPU | AMD Radeon 860M (integrated), 4.0 GB VRAM, DirectML | Alder Lake-N UHD Graphics (integrated), Vulkan, 0.0 GB dedicated VRAM reported |
| Network | Wi-Fi | unknown (not confirmed Ethernet vs. Wi-Fi) |

ICMP round-trip between the two over this LAN measured 8-14ms
(`ping Desk-Mini`), consistent with the TCP bridge-write latency below.

**A real capacity-modeling caveat, stated plainly**: `print_flow` (the code
behind the `flow` subcommand this harness drives) hardcodes the local
node's declared capacity to a minimum of 16GB
(`crates/ghost-link/src/main.rs`, `print_flow`, `.max(16.0)`) for its fixed
60-layer/30GB synthetic scenario — that's a demo-scenario convenience, not
a claim about this laptop's real ~4GB VRAM. Desk-Mini's real probed
capacity (`ghost-link probe local-node`) is 0.0 GB dedicated VRAM / 14.9 GB
system RAM — an integrated-GPU mini PC that could not actually hold a
meaningful share of a real 30B-class model's weights. Passing that real
number to `--remote-vram-gb` produces a degenerate all-or-nothing
placement (whichever side has more declared capacity absorbs the entire
synthetic workload, leaving the other with zero assigned stages — this
also happened during setup with the *opposite* value: `--remote-vram-gb
32` handed all 60 layers to Desk-Mini and none to the coordinator, which
then hard-errored). `--remote-vram-gb 16` was used here as a **declared
test input to force a genuine two-node split**, not as a claim about
Desk-Mini's real capability. This is the same limitation
[DEPLOYMENT.md's Stage 3b](DEPLOYMENT.md#stage-3b-real-cross-machine-flow-execution)
already documents: per-stage compute is a synthetic timing proxy, not real
model math — what this benchmark actually validates is the **real
cross-machine transport** (genuine `TcpStream::connect`, real framed
batches, real measured round-trip latency), not real distributed LLM
throughput.

```bash
python scripts/remote_flow_benchmark.py --remote-addr <desk-mini-ip>:9500 \
  --runs 5 --release --remote-vram-gb 16 --remote-mem-gb 14.9
```

(worker side, restarted fresh before each one-shot run:
`GHOSTLINK_TCP_AUTH_TOKEN=local-token ghost-link stage-worker 0.0.0.0:9500`)

| Runs | Throughput (avg) | Throughput (min-max) | P95 (avg) | Remote bridge-write (avg) | Remote bridge-write (min-max) |
|---|---|---|---|---|---|
| 5 | 337.4 tok/s | 253.4-385.2 tok/s | 19.50 ms | 12.11 ms | 10.37-15.77 ms |

Every run's `stage_stats` confirms 2 pipeline stages — stage 0 (local,
compute-only, ~0.008ms, zero bridge-write) and stage 1 (Desk-Mini, zero
compute, 10.4-15.8ms bridge-write) — meaning every run genuinely crossed
the real network for its remote stage, not a loopback shortcut. Raw output:
`tmp/remote_flow_benchmark/` (`summary.json` + one `remote-N.json` per run).

The absolute throughput number here (`tokens/sec` of the synthetic pipeline
harness) isn't comparable to the Full-Spectrum session's in-process numbers
above — different code path, different hardware pair, and a much lower
`exec_tokens`/`micro_batch` (this harness's defaults: 256/4). The real
signal is `remote_bridge_write_ms`: a genuine ~10-16ms round-trip write
cost to a physically separate machine over Wi-Fi, in the same range as raw
ICMP RTT to that host.

### Notes on Multi-Node Performance

- The one real run above used exactly 2 nodes; scaling trends across 3+
  nodes are still untested hypotheses, not measurements.
- Bridge-write latency here tracks network RTT closely (10-16ms TCP vs.
  8-14ms ICMP) — on this LAN, transport overhead is dominated by the
  network hop itself, not Ghostlink's framing/serialization.

---

## Memory Requirements

Reference values for common quantization sizes, not measured on Ghostlink
hardware — use these for capacity planning, not as a Ghostlink-specific
throughput/latency claim.

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

This section previously described a generic test configuration (a
24-core/RTX 4090 desktop, synthetic 10K-token prompts) that no number in
this document was actually produced on or with — it's been replaced with
the methodology the real entries above actually use.

### Tools

- `cargo bench -p ghostlink-core --bench criterion` — primitive-level
  microbenchmarks (ring buffer, protocol encode/decode, planning, cluster).
- `scripts/flow_perf_snapshot.py` — full pipeline throughput/latency,
  matched against `docs/PERF_BASELINE.json` / `PERF_BASELINE_STRESS.json`
  for drift detection (`scripts/check_perf_drift.py`).
- `scripts/remote_flow_benchmark.py` — real cross-process/cross-machine
  `stage-worker` benchmark harness.
- Full invocations and real output are in the Full-Spectrum Session
  Benchmark above; see [TESTING.md](TESTING.md) for the broader test/bench
  workflow this repo expects before a release.

### Test Procedure

1. Build in release mode (`cargo build --release`; workspace already sets
   `lto = "thin"`, `codegen-units = 1`).
2. Run the tool for the layer being measured (primitive vs. full pipeline
   vs. cross-process), matching `exec_tokens`/`micro_batch` to whatever
   baseline you're comparing against — mismatched parameters produce a
   number that looks like drift but isn't (see the Full-Spectrum session's
   own note on this).
3. Prefer multiple interleaved runs over a single run before drawing any
   conclusion — this repo's own dev hardware showed 5-80% run-to-run swings
   on some benchmarks (see "Methodology note: this host is noisy" above).

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

Untested hypotheses pending the real multi-node LAN benchmark above — not
yet backed by measurement on this project:

1. Faster networks should help more nodes scale better.
2. Fewer nodes should mean less network overhead.
3. Balancing layers evenly across nodes should improve utilization.

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

- [README.md](../README.md) - Project overview, installation, and headline
  Performance numbers
- [ROADMAP.md](ROADMAP.md) - Competitive strategy, including the real
  distributed-inference work this document's Multi-Node section depends on
- [DEPLOYMENT.md](DEPLOYMENT.md) - Cross-machine deployment, including the
  `stage-worker` path used by `scripts/remote_flow_benchmark.py`
- [TESTING.md](TESTING.md) - Full test/bench workflow
