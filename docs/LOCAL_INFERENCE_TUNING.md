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
| mlock | `--mlock` | off by default on ≤24GB RAM hosts |

Override anytime:

```bash
export GHOSTLINK_LLAMA_SERVER_ARGS="-fa on -b 2048 -ub 512"
export GHOSTLINK_VRAM_GB=12
export GHOSTLINK_LLAMA_NGL=40
```

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
