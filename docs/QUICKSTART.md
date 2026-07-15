# Quickstart

## One-command launch

**Windows:**
```powershell
launch-complete.bat
```

**Linux/macOS:**
```bash
bash launch-complete.sh
```

The script:
1. Detects hardware (CUDA/Metal/DirectML/NPU/CPU)
2. Builds the Rust backend (`cargo build`)
3. Installs npm dependencies
4. Starts the backend API on `http://127.0.0.1:8003`
5. Starts Vite dev server on `http://127.0.0.1:5173`
6. Opens the GUI

## What you can do

1. **Chat** — Select a model, send messages with SSE streaming
2. **Download models** — Browse Hugging Face tab, click Download; progress shows in Library tab
3. **Monitor metrics** — CPU/GPU/memory utilization, active sessions
4. **Cluster workers** — Add remote workers, discover peers on LAN
5. **Settings** — Configure ports, discovery, auth tokens

## Default models

The backend starts with two tiny models (safe for low-RAM systems):
- `stories15M` (15M params, ~8 MB)
- `TinyLlama-1.1B-Chat` (1.1B params, ~650 MB)

Download larger models from the Hugging Face tab in the GUI.

## Environment shortcuts

```powershell
# Skip model download
set GHOSTLINK_SKIP_MODEL=1 && launch-complete.bat

# Skip Rust build
set GHOSTLINK_SKIP_BUILD=1 && launch-complete.bat

# Use ollama backend instead of native llama-server
set GHOSTLINK_INFERENCE_BACKEND=ollama && launch-complete.bat
```
