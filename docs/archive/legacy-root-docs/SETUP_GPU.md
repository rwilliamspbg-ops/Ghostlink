# GPU Acceleration Setup

## Overview

Ghostlink is fully optimized for GPU-accelerated inference using AMD ROCm on AMD Radeon 860M GPU.

## GPU Configuration

The launcher (`launch-ollama.bat`) automatically configures:

```
OLLAMA_NUM_THREAD=16              # All 16 CPU cores
OLLAMA_GPU_MEMORY=3276            # 80% safe allocation (4GB VRAM)
HIP_PLATFORM=amd                  # AMD ROCm backend
HSA_OVERRIDE_GFX_VERSION=gfx906   # GPU architecture mapping
OLLAMA_IGPU_ENABLE=1              # Enable integrated GPU
OLLAMA_BATCH_SIZE=512             # Performance optimization
OLLAMA_CACHE_SIZE=2048            # Inference cache
```

## Performance

- **Inference Speed**: ~50 tokens/sec per model (GPU-accelerated)
- **GPU Utilization**: 20-40% during inference
- **Model Capacity**: 10+ models loaded and switchable
- **Cold Start**: ~3.9 seconds (model load to GPU)
- **Warm Cached**: ~0.5-1 second (model in VRAM)

## Running Ghostlink

```bash
C:\Users\rwill\Ghostlink\launch-ollama.bat
```

Services start automatically:
- Ollama GPU inference: `http://127.0.0.1:11434`
- Backend API: `http://127.0.0.1:8003`
- Chat GUI: `http://127.0.0.1:5173` (auto-opens)

## GPU Troubleshooting

If Ollama doesn't detect GPU:

1. Check Ollama window output for: `library=ROCm` and `compute=gfx906`
2. If missing, verify AMD drivers are installed
3. If gfx906 mapping fails, try alternative GFX targets:
   - `gfx1030`, `gfx1050`, `gfx1100`, or `gfx906`
4. Edit `launch-ollama.bat` and change line with `HSA_OVERRIDE_GFX_VERSION`

If Ollama detects GPU but generation fails with `POST /api/generate` returning `404`:

1. Confirm Ollama is reachable and has tags:
   - `curl http://127.0.0.1:11434/api/tags`
2. List installed models and verify the exact tag:
   - `ollama list`
3. Pull the exact model tag you will use:
   - `ollama pull qwen2.5:3b`
4. In Ghostlink, select the exact installed tag (including suffix like `:latest` when present).
5. If logs show warning about overridden visible devices, clear override and restart:
   - Windows: `setx HSA_OVERRIDE_GFX_VERSION ""`
   - Linux/macOS: `unset HSA_OVERRIDE_GFX_VERSION`

## Known Limitations

**Model Switching**: Browser reload may still be needed for some UI state changes, but backend/runtime selection is now available through the Settings tab and API.

## Architecture

- **CPU**: AMD Ryzen AI 7 350 (16 cores, 100% utilized)
- **GPU**: AMD Radeon 860M (14.2 GiB available for models)
- **RAM**: 28 GB system memory
- **Backend**: Ollama + ROCm + HIP acceleration
