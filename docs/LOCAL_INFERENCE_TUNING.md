# Local Inference Tuning

How to push prompt-eval and decode throughput higher on local Ghostlink nodes
(llama.cpp / llama-server).

## Defaults applied by Ghostlink

Launch paths (`launch.sh`, `launch-complete.sh`, `launch-ollama.bat`) and the
native engine (`native_engine.rs`) now enable:

| Knob | Flag | Default (VRAM-scaled) |
|------|------|------------------------|
| Context size | `-c` | **16384** when `GHOSTLINK_VRAM_GB` is unset, else **8192** on 8GB up to 32768 on 16GB+ (never the model default 128k); further capped down for large (≥10GB) models regardless of VRAM tier — see [GPU offload and large models](#gpu-offload-and-large-models--read-this-before-setting--ngl--1-on-an-igpu) below, and `GHOSTLINK_CTX_SIZE`/`GHOSTLINK_VRAM_GB` in `native_engine::get_ctx_size` |
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
backend, Qwen3-Coder-30B-A3B-Instruct-Q3_K_L, **`-ngl -1`**): raising `-c`
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

Note the `-ngl -1` in that measurement, because it is load-bearing: it is the
configuration the failure belongs to, and it is *not* the one the policy picks for
these models — `get_ngl` forces `-ngl 0` for anything ≥10 GB (see the decode
regression documented in `native_engine::get_ngl`). CPU-only, the constraint is
system RAM rather than the device heap, and the same 12.11 GB model served 32768
ctx with 8.1 GB free. So this failure bounds the `-ngl -1` case; it does not
bound the `-ngl 0` case that actually runs.

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

## 0. Thread count on a hybrid CPU

On a hybrid CPU, half the logical processors are efficiency cores. llama.cpp schedules
across whatever `-t` it is given, so passing every logical processor means E-cores
participate in the matmul loop.

Measured on this project's reference host — AMD Ryzen AI 7 350 (Zen 5, Radeon 860M).
`GetLogicalProcessorInformationEx` reports 8 processor-core records with
`EfficiencyClass = [1,0,1,0,1,0,1,0]`, i.e. 4 performance and 4 efficiency cores,
8 physical / 16 logical. With the 3B model at `-ngl 0` so the question is genuinely
CPU-bound, 4 runs per config, medians:

| `-t` | prefill | decode |
|---|---|---|
| 4 | 254.2 tok/s | 22.36 tok/s |
| 8 (P-cores + SMT) | 254.1 tok/s | 21.55 tok/s |
| 16 (all logical) | 246.2 tok/s | 20.18 tok/s |

**1.07x decode** for P-cores-only versus all logical processors. Modest — llama.cpp's
own thread scaling is good, so this is not a 2x lever — but it is real, free, and it was
previously not being applied at all.

`-t 8` is the right value, not `-t 4`: `performance_cores` is a *physical* count, so it
is scaled by `logical / physical` to keep SMT threads on the performance cores.
Limiting to the physical P-core count alone would leave half the FP throughput unused.

Worth noting: this host's topology is not obvious from its name. "Ryzen AI 7 350"
sounds uniform-core, but the OS reports a genuine 4P/4E split. Read it rather than
assuming — which is what `SystemProfile::cpu.performance_cores` is for, and it was
already being computed on every probe and never consulted.

## 1b. The context ceiling dominates long conversations

`get_ctx_size` caps context by model size, independently of the VRAM tier. Note
that the result is `min(vram_tier, model_cap)`, and when `GHOSTLINK_VRAM_GB` is
unset — a direct `ghost-link serve`, which is how this install runs — the flat
tier (16384) is what binds:

| model size | `model_cap` | ctx granted (VRAM unset) | turns of ~500 tokens |
|---|---|---|---|
| under 5 GB | none | 16384 | ~32 |
| 5-10 GB | 32768 | 16384 | ~32 |
| 10 GB and over | **8192** | **8192** | **~16** |

Measured on this project's reference hardware, where the configured model is
5.4 GiB and therefore gets 16384 — 4096 per slot at the `-np 2` that
`settings.json` sets.

This is separate from model choice. A faster model does not help if the context is capped,
and a larger context does not help if the model cannot be loaded at all (see
`scripts/model_survey.py`, which records that both models over 10 GB currently fail to
start on this device).

Unlike `get_ngl`, the `model_cap` table used to carry no measurement or comment
explaining where 4096 came from — a plausible-looking constant. It now does; see
the re-measurement below. `GHOSTLINK_CTX_SIZE` still overrides it, but only when
`ctx_size_auto` is false — and a non-default value with `ctx_size_auto: true`
looks configured while doing nothing, which is what the `[perf-tier] IGNORED`
line at model load reports.

### Measured: both caps were stricter than the hardware requires

`scripts/ctx_sweep.py` (throughput) and `scripts/ctx_memory_sweep.py` (memory, the
axis a context cap actually guards) on the reference hardware, `-np 2` and
`-fa on -ctk/-ctv q8_0`:

| model | `-ngl` | cap | served | KV cost | free RAM after load |
|---|---|---|---|---|---|
| Qwen3.8-9B, 5.78 GB | -1 | 8192 | **65536** | 34.0 KiB/cell, linear | 13.8 GB |
| gpt-oss-20b, 12.11 GB | 0 | 4096 | **32768** | 6 MiB, **flat** (sliding window) | 8.1 GB |

Throughput barely moves over that range: 10x the context cost ~5-7% on the 9B
and ~0% on the 12 GB model. So neither cap was protecting throughput.

### The size tier cannot see KV cost, and that is why the caps stay conservative

The same sweep run earlier against a *dense* 12.04 GB model failed outright:

| ctx | outcome |
|---|---|
| 4096 | usable -- 124s load, 17.2 prefill, 1.95 decode |
| 8192 | usable -- 122s load, 17.1 prefill, 1.95 decode (1.00x decode) |
| 10240 | usable -- 120s load, 17.3 prefill, 1.97 decode |
| 12288 | never serves |
| 14336 | never serves |

Two models of ~12 GB, in the same tier, with ceilings **2.7x apart** — 12288
versus 32768 — because one is dense and one uses sliding-window attention, so
their KV costs differ by orders of magnitude. Repeat sweeps also disagreed at the
same size on the dense model (`8192: 1 of 2 trials usable`), with
`vk::Device::allocateMemory: ErrorOutOfDeviceMemory` on a shared iGPU.

That is the whole argument for not tracking the measurement: **a file-size table
cannot see a model's KV cost.** A cap raised to the ceiling measured for one model
is unsafe for a model whose KV is several times more expensive per cell, and the
documented OOM — a 13.6 GB model at 16384 ctx leaving a 27.6 GB host under 1 GB
free — is exactly that case.

So the caps were doubled rather than maximised, and the `>=10 GB` cap stays
*below* the 16384 that OOM'd:

| tier | before | after | measured ceiling |
|---|---|---|---|
| VRAM unset (flat tier) | 8192 | **16384** | — |
| `model_cap` 5-10 GB | 8192 | **32768** | 65536 |
| `model_cap` >=10 GB | 4096 | **8192** | 32768 |

Raising `model_cap` alone would have changed nothing in practice: the result is
`min(vram_tier, model_cap)`, so the flat 8192 tier was the binding value for every
model. Both had to move together.

### Closing the gap properly needs the model's KV dimensions

The honest fix is to derive ctx from available memory and the model's actual KV
bytes/token, read from the GGUF header (`n_layer`, `n_head_kv`, head dim) before
launch — the codebase currently reads only `fs::metadata` file size, so that is
new machinery, not a constant. Until then the tier is a size proxy, and it is
deliberately loose rather than tight.

## 1. Prompt processing / ingestion

If you have free VRAM, increase batch sizes:

```text
-b 2048 -ub 512
```

**Measured on this project's reference hardware, not quoted from upstream.** AMD
Radeon 860M (Vulkan, 16.4GB), Llama-3.2-3B Q3_K_M, `-ngl 24 -fa on -ctk/-ctv q8_0`,
3,372-token prompt where prefill dominates, 4 runs per config, medians:

| config | prefill | decode |
|---|---|---|
| `-b 512 -ub 128` | 303.0 tok/s | 12.55 tok/s |
| `-b 2048 -ub 512` | 327.2 tok/s | 12.77 tok/s |
| `-b 2048 -ub 2048` | 321.0 tok/s | 12.71 tok/s |

So **+8% prefill**, not the "~70 t/s → 300+ t/s" that upstream guidance and several
secondary sources suggest. That figure is presumably about discrete-GPU CUDA with a
much larger batch, and it does not reproduce here. `-ub 2048` is slightly *worse* than
`-ub 512`, so there is no benefit to raising it further.

Note the prompt size matters for how this looks. At 893 prompt tokens the same
`-b 2048 -ub 512` change measured as a *regression* (0.62x prefill, 3.04x slower
decode), because at that size wall time is dominated by decode and the larger batch
just holds more VRAM. Report prefill and decode separately or this lever looks like
noise in both directions.

Reproduce with `python scripts/llama_knob_ab.py`, which A/Bs these and the KV-cache
and Flash Attention settings on one node with one variable at a time.

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


## Speculative Decoding & Tensor Pinning

### Speculative Decoding
Speculative decoding uses a smaller draft model to accelerate text generation.
Configure draft knobs via environment variables:
- `GHOSTLINK_DRAFT_MODEL`: Path to the draft model file (`.gguf`).
- `GHOSTLINK_DRAFT_MAX`: Number of tokens to draft per step (e.g. `16`).
- `GHOSTLINK_DRAFT_P_MIN`: Minimum probability threshold for accepting draft tokens (e.g. `0.2`).

When `GHOSTLINK_DRAFT_MODEL` is set and points to a valid file, Ghostlink passes
`--spec-draft-model <path>`, `--spec-draft-n-max`, and `--spec-draft-p-min` to
`llama-server`.

Two corrections to this section as it previously stood:

- **Nothing read `GHOSTLINK_DRAFT_MODEL` at all.** The variables were documented here
  with a specific promise about which flags were passed, and no code in the repository
  consumed any of them. Setting them did nothing.
- **`--draft-max` no longer exists.** This build's `llama-server --help` reports:

  ```text
  --draft, --draft-n, --draft-max N   the argument has been removed.
                                     use --spec-draft-n-max or ...
  ```

  Passing the documented `--draft-max` would make `llama-server` exit on an unknown
  argument. The names above are the ones this build parses; re-check against
  `llama-server --help` after any llama.cpp bump.

Not enabled by default and not benchmarked here. A draft model close in size to the
target costs more than it saves, and the right pairing is model-specific.

### RPC Tensor Pinning & "RPC Only If It Does Not Fit"
- **Fit-Local Rule**: RPC distribution is automatically bypassed if the model plus KV cache fits completely within local VRAM. Same-host or unnecessary Ethernet RPC is a performance regression. Set `GHOSTLINK_REQUIRE_CLUSTER_OFFLOAD=1` if you want to force RPC offloading regardless.
- **Tensor Pinning (`-ot`)**: When RPC is used, Ghostlink pins latency-critical tensors locally (attention, KV cache, embeddings, output/lm_head) on the local accelerator and routes heavy FFN / MoE expert tensors to remote RPC workers via `-ot ffn=RPC,exps=RPC`. Set `GHOSTLINK_LLAMA_OVERRIDE_TENSOR` to override this tensor layout mapping.

## Quick verify

```bash
# After launch, check llama-server was started with perf flags
# Linux:  grep -E 'fa|batch' /tmp/ghostlink_llama_server.log
# Or inspect the model-load log line from ghost-link:
#   [model-load] Command: ... -fa on -b 1024 -ub 512 ...
```


## Distributed Offload Thresholds & Model-Ready Timeout Scaling

### Distributed Offload Guard
When `distributed_inference: true` is set, Ghostlink checks if the distributed launch is meaningful before attaching `--rpc` and `-ts` flags:
- **-ngl > 0**: If `-ngl` is set to `0` (CPU-only), model layers stay on the coordinator and no layers leave for remote RPC peers.
- **Remote Share >= 1%**: If calculated remote tensor split share is below 1% (`MIN_REMOTE_SHARE_THRESHOLD = 0.01`), RPC communication overhead outweighs any offload benefit.

When either condition fails:
- Ghostlink logs a warning (`tracing::warn!`), sets `distributed_active: false` in the placement plan, and populates `summary_text` with an explicit explanation.
- By default, Ghostlink falls back to single-node local execution so inference succeeds.
- If `GHOSTLINK_REQUIRE_CLUSTER_OFFLOAD=1` or `require_cluster_offload: true` in settings, Ghostlink fails the load outright with a 400 error.

### Model-Ready Timeout Scaling
Cross-machine RPC model loads take significantly longer than single-node loads, scaling with model file size and peer count.

Ghostlink dynamically calculates the `llama-server` model-ready timeout as:
$$\text{timeout\_secs} = \text{clamp}(90\text{s (floor)} + (\text{model\_size\_gb} \times 15\text{s}) + (\text{peer\_count} \times 60\text{s}), 90\text{s}, 1800\text{s})$$

- **Floor**: 90s minimum.
- **Cap**: 1800s (30 minutes) maximum.
- **Override**: Set `GHOSTLINK_MODEL_READY_TIMEOUT_SECS=<seconds>` to force a specific timeout.


## Context Budgeting (actual behaviour)

Ghostlink's context handling is **not** a typed policy system. What exists today:

### Chat path (`handle_gui_chat` → `native_engine.rs` / `ollama.rs` / `vllm.rs`)

- `conversation_token_limit`: max tokens allowed for conversation history, applied by
  `trim_conversation_history_async`, which walks prior turns newest-first and keeps
  whatever fits, dropping older turns. Token cost uses the native tokenizer when the
  native engine is active and a whitespace-word estimate otherwise. There is **no**
  reserved completion budget — the limit is spent entirely on history.
- `kv_cache_type` (`-ctk`/`-ctv`) and `max_tokens` (response cap) are real settings.
- **No** `sliding_window` / `truncate_oldest` / `compact` policies, **no**
  summarization or memory-blob, **no** sticky-slot (`id_slot`) pinning, and **no**
  `n_ctx` / `n_ctx_auto` settings. This section previously documented all of those as
  implemented; they are not present in the codebase.

### Task-agent path (`task_runtime.rs`)

- `ContextGovernor` (real, added 2026-10-02) packs the agent transcript to fit a token
  budget before every backend call, pinning the system prompt and the original task
  goal and keeping the most recent turns. Compaction is surfaced as a `risks[]` entry
  on the `ReviewPacket` and a `context_compacted` SSE event, so a reviewer knows the
  agent was working from a truncated transcript.

### Memory budgeting for large / distributed models

The claims that previously followed here — that large models receive a VRAM-based
"context ceiling", that distributed RPC lifts a hard 4096-token cap, and that
`load_model_into_slot` returns a structured memory-fit error — were **not verified**
and should not be relied on. What is real: `native_engine.rs` computes a
model-ready timeout from model size and RPC peer count
(`compute_model_ready_timeout`), and `rpc_cluster::compute_tensor_split` weights the
tensor split by declared capacity.
