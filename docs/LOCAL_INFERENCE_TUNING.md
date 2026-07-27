# Local Inference Tuning

How to push prompt-eval and decode throughput higher on local Ghostlink nodes
(llama.cpp / llama-server).

## Defaults applied by Ghostlink

Launch paths (`launch.sh`, `launch-complete.sh`, `launch-ollama.bat`) and the
native engine (`native_engine.rs`) now enable:

| Knob | Flag | Default (VRAM-scaled) |
|------|------|------------------------|
| Context size | `-c` | **8192** on 8GB, up to 32768 on 16GB+ (never model default 128k) — see `GHOSTLINK_CTX_SIZE`/`GHOSTLINK_VRAM_GB` in `native_engine::get_ctx_size` |
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
