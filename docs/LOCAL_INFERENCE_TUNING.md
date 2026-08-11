# Local Inference Tuning

How to push prompt-eval and decode throughput higher on local Ghostlink nodes
(llama.cpp / llama-server).

## Defaults applied by Ghostlink

Launch paths (`launch.sh`, `launch-complete.sh`, `launch-ollama.bat`) and the
native engine (`native_engine.rs`) now enable:

| Knob | Flag | Default (VRAM-scaled) |
|------|------|------------------------|
| Context size | `-c` | **8192** on 8GB, up to 32768 on 16GB+ (never model default 128k); further capped down for large (≥10GB) models regardless of VRAM tier — see [GPU offload and large models](#gpu-offload-and-large-models--read-this-before-setting--ngl--1-on-an-igpu) below, and `GHOSTLINK_CTX_SIZE`/`GHOSTLINK_VRAM_GB` in `native_engine::get_ctx_size` |
| KV cache type | `-ctk/-ctv q8_0` | compact KV (~2× less cache memory) |
| Flash Attention | `-fa on` | always on (requires value on current llama.cpp) |
| Batch size | `-b` | 2048 (≥12GB) / 1024 (≥8GB) / 512 (else) |
| µ-batch size | `-ub` | 512 (≥8GB) / 256 (≥4GB) / 128 (else) |
| mlock | `--mlock` | off unless `GHOSTLINK_MLOCK=1` or `GHOSTLINK_SYSTEM_MEMORY_GB>=24` — see below |
| mmap | `--no-mmap` | off (mmap stays on) unless `GHOSTLINK_NO_MMAP=1` |

Override anytime:

```bash
export GHOSTLINK_LLAMA_SERVER_ARGS="-fa on -b 2048 -ub 512"
export GHOSTLINK_VRAM_GB=12
export GHOSTLINK_LLAMA_NGL=40
export GHOSTLINK_MLOCK=1        # or 0 to force off; unset = RAM-tier default
export GHOSTLINK_NO_MMAP=1      # opt-in only, no default heuristic
```

`--mlock`/`--no-mmap` used to only be set by the shell launch scripts
(`launch.sh`), so any model load that went through the Rust engine directly
(GUI, API, `launch-native.ps1`) never got them. `native_engine.rs`'s
`get_mlock()`/`get_no_mmap()` now set them the same way `launch.sh` already
did: `GHOSTLINK_MLOCK` wins outright if set; otherwise mlock defaults on only
when `GHOSTLINK_SYSTEM_MEMORY_GB>=24` (unset → off, since guessing wrong
about a memory-tight host is worse than skipping mlock). `--no-mmap` has no
default heuristic — it's opt-in only via `GHOSTLINK_NO_MMAP=1`, since nothing
in this repo has measured a throughput case for turning it on by default.

## GPU offload and large models — read this before setting `-ngl -1` on an iGPU

`native_engine::get_ngl` (Rust) and `launch.sh`'s own shell tiering both cap
**large models (≥10GB) toward CPU-only (`ngl=0`) by default** when
`GHOSTLINK_LLAMA_NGL` isn't set — this is deliberate, not an oversight, and
`GHOSTLINK_LLAMA_NGL` still wins outright as an explicit override either way.

Why: on an integrated GPU (Vulkan backend — AMD/Intel iGPUs on Windows and
Linux), "VRAM" is the same physical RAM as everything else. Unlike a
discrete GPU, where offloading a layer *moves* its weights out of system RAM
into separate VRAM, llama.cpp's Vulkan backend on this class of hardware
**duplicates** offloaded weights into a separate device-local allocation
instead. Measured directly (AMD Radeon 860M, 27.6GB host, 13.6GB model,
controlled comparison — same prompt/seed, same direct llama-server timings,
`ngl` 0 vs 24 vs -1):

| `ngl` | decode speed | committed memory | free system RAM while loaded |
|---|---|---|---|
| `0` (CPU-only) | 8.78 tok/s | 0.54GB | ~18GB |
| `24` (partial offload) | 8.03 tok/s | 7.35GB | ~11GB — **not a viable middle ground**, same or worse speed than 0 for 13x the memory |
| `-1` (full offload) | 16.84 tok/s | 14.15GB | ~0.4GB — reproduced twice |

Full offload really is ~1.9x faster, but leaves well under 1GB free for
everything else on the host while a model that size is loaded — real,
reproducible, not a hypothetical. CUDA (real discrete NVIDIA VRAM) and ROCm
don't have this problem, so the automatic large-model cap only applies to
the Vulkan backend; it doesn't second-guess a CUDA/ROCm setup. Apple Metal's
behavior here is unverified (not tested on this hardware) and is
deliberately *not* capped by this logic — Metal's buffer model may not have
the same duplication behavior, and claiming otherwise without measuring it
would just be a guess.

If you want full offload on an iGPU anyway (e.g. you've measured your own
tradeoff and it's worth it, like `launch-native.ps1`'s reference machine
does), set `GHOSTLINK_LLAMA_NGL=-1` yourself — that's a permanent, explicit
opt-in, not something to leave to chance on a host you haven't measured.

### iGPU VRAM estimate on `launch.sh`

`detect_gpu()` has no way to read a real "VRAM" figure for an AMD/Intel iGPU
(there isn't one — it's shared system RAM), so `launch.sh` estimates a tier
instead of always falling back to CPU-only: **1/3 of total system RAM,
capped at 8GB**, applied only when the Vulkan backend was detected and
`GHOSTLINK_VRAM_GB` wasn't set explicitly. This is a tuning-tier input, not
a memory reservation — the actual danger case (a large model on this
backend) is already handled by the model-size cap above regardless of what
this estimate lands on, so it can afford to be reasonably generous rather
than maximally conservative. `GHOSTLINK_VRAM_GB` always wins outright when
set.

### Context length is capped by the same device-local heap as `-ngl -1`, not just system RAM

Measured directly on the reference hardware (AMD Radeon 860M, Vulkan
backend, Qwen3-Coder-30B-A3B-Instruct-Q3_K_L, `-ngl -1`): raising `-c`
past its auto-tiered default (4096 for this model) fails to load —
`ggml_vulkan: Device memory allocation ... failed: ErrorOutOfDeviceMemory`
— even from `+25%` (5120) and even with **9.6GB of general system RAM
free at the time**. This is not a system-memory question: full offload's
weight buffers already leave the Vulkan device-local heap essentially
full, so *any* extra KV-cache allocation for a longer context fails
regardless of how much ordinary RAM is free. Tried at three KV cache
types (`q8_0`, `q4_0`, and the auto-retry's unquantized `f16` fallback)
— all failed identically, confirming it's the extra allocation itself,
not which quant it's made of.

Practical takeaway: on this class of hardware, full GPU offload and a
context window bigger than the auto-tiered default are mutually
exclusive for a model this size — there's no safe middle ground to tune
into via `-c`/`-ctk`/`-ctv` alone. Getting more context headroom means
giving up device-local heap somewhere else: lower `-ngl` (accepting the
speed cost documented above), a smaller/more-quantized model, or fewer
concurrently-loaded buffers. Confirmed safe throughout testing: a failed
load attempt leaves the previously-running server untouched (staging
loads the new config on a scratch port and only swaps over once it's
confirmed healthy — see `NativeEngineClient::load_model_into_slot`), so
experimenting with this is low-risk even on a memory-tight host.

## Hybrid CPU (P/E-core) detection

`SystemProfile::detect()`/`detect_fast()` now report `cpu.performance_cores`
/`cpu.efficiency_cores` on Windows via `GetLogicalProcessorInformationEx`'s
per-core `EfficiencyClass` — `None`/`None` when the CPU has no real
heterogeneous split (still the common case) or on non-Windows (no
equivalent probe implemented yet). Verified live on this repo's own AMD
Ryzen AI 7 350 dev machine: `physical_cores=8` splits as
`performance_cores=Some(4)` / `efficiency_cores=Some(4)` — AMD's "Strix
Point" mobile chips genuinely mix full Zen5 and compact Zen5c cores, this
isn't Intel-only.

This is detection only — `native_engine.rs`'s `get_threads()` does **not**
yet default `-t` to `performance_cores` on a hybrid CPU. Intel's P/E split
has a large well-documented per-thread speed gap (Atom-derived E-cores),
where capping threads to P-cores is a known win because a synchronized
thread pool is only as fast as its slowest thread. AMD's Zen5c cores are
the same microarchitecture at a lower clock, a much smaller gap — whether
capping helps, hurts, or does nothing on this specific chip hasn't been
measured, and guessing wrong here would regress CPU-bound throughput
instead of improving it. Until that's measured, set
`GHOSTLINK_LLAMA_THREADS` yourself to experiment (e.g. `=4` to test
P-core-only) — the explicit override always wins over anything an
autodetected default would pick anyway.

## Distributed (`--rpc`) peer health

`rpc_cluster::discover_rpc_peers` now excludes a peer whose
`delivery_ratio` metric has dropped below `0.90`, even if it's still
`NodeStatus::Active` (heartbeat still arriving) — a struggling-but-alive
peer can hurt overall throughput more than dropping it. A peer with no
samples yet defaults to `delivery_ratio = 1.0`, so freshly-joined nodes are
never penalized before they have real data. This does *not* also gate on
observed latency: `NodeMetrics::avg_latency_us` is populated with
millisecond-scale values in some real call sites despite the field's name,
so a fixed cutoff there risks silently excluding every peer (or none)
depending on which unit actually applies — not applied until it's measured
against this real path specifically.

Docker Compose uses:

```yaml
LLAMA_ARG_FLASH_ATTN=on
LLAMA_ARG_BATCH=2048
LLAMA_ARG_UBATCH=512
```

## 1. Prompt processing / ingestion

If you have free VRAM, increase batch sizes:

```text
-b 2048 -ub 512
```

Typical effect on modern GPUs: prompt eval ~70 t/s → 300+ t/s.

## 2. Flash Attention

Always pass `-fa on` (or `LLAMA_ARG_FLASH_ATTN=on`) so long-context decode is not
memory-bandwidth bound. Bare `-fa` is rejected by current llama.cpp builds.

## 3. Quantization precision

For local execution loops, prefer:

- **Q4_K_M** (default catalog preference)
- **IQ4_XS** (slightly smaller / often faster)

Avoid as primary local quants when speed matters:

- FP16 / BF16
- Q8_0

Expect roughly **1.5x–2x** token-generation speedup vs FP16/Q8 with little
quality loss for agent/tool loops.

This repo already ships `models/gemma-4-E4B-it-Q4_K_M.gguf` as the preferred
Windows launch model.

## Quick verify

```bash
# After launch, check llama-server was started with perf flags
# Linux:  grep -E 'fa|batch' /tmp/ghostlink_llama_server.log
# Or inspect the model-load log line from ghost-link:
#   [model-load] Command: ... -fa on -b 1024 -ub 512 ...
```
