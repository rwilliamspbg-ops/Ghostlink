# Ghostlink Studio

Ghostlink Studio is a full-stack local AI workspace with:
- a Rust backend API
- a modern React GUI
- optional Ollama inference integration
- model/session/worker management surfaces

## At A Glance

- Backend API: http://127.0.0.1:8003
- GUI: http://127.0.0.1:5173
- Ollama (optional): http://127.0.0.1:11434

## Unified Launch Paths

All launch scripts are now aligned to the same default ports and full-stack behavior.

| Script | Platform | Starts Backend | Starts GUI | Starts Ollama | Notes |
|---|---|---:|---:|---:|---|
| `launch-complete.sh` | Linux/macOS | Yes | Yes | If needed | Canonical full-stack launcher |
| `launch-splash.sh` | Linux/macOS | Yes | Yes | If needed | Splash + delegates to `launch-complete.sh` |
| `launch.sh` | Linux/macOS | Yes | Yes | Launcher-managed | Delegates to `scripts/launch_studio.sh` |
| `launch-complete.bat` | Windows | Yes | Yes | Yes | Full-stack launcher in separate consoles |
| `launch-splash.bat` | Windows | Yes | Yes | Yes | Splash + delegates to `launch-complete.bat` |
| `launch.bat` | Windows | Yes | Yes | Launcher-managed | Delegates to `scripts/launch_studio.bat` |
| `ghostlink_gui_modern/launch-gui.sh` | Linux/macOS | No | Yes | No | Frontend-only launcher |
| `ghostlink_gui_modern/launch-gui.bat` | Windows | No | Yes | No | Frontend-only launcher |

## Quick Start

### Linux/macOS

```bash
bash launch-complete.sh
```

### Windows

```bat
launch-complete.bat
```

The launcher will:
1. Build and start backend on `127.0.0.1:8003`.
2. Start GUI on `127.0.0.1:5173`.
3. Start or reuse Ollama when available.
4. Keep services running until `Ctrl+C`.

## Manual Start (Alternative)

### Terminal 1: backend

```bash
cargo run -p ghost-link -- serve 127.0.0.1 8003
```

### Terminal 2: GUI

```bash
cd ghostlink_gui_modern
npm install --legacy-peer-deps
npm run dev -- --host 127.0.0.1 --port 5173
```

### Terminal 3 (optional): Ollama

```bash
ollama serve
```

## Native Llama Server Mode

Use this path to run Ghostlink with native local inference (no Ollama dependency in request path).

### One-command stack launch

```bash
bash scripts/run_native_llama_server_stack.sh
```

This script will:
1. Ensure `llama.cpp` exists under `third_party/llama.cpp`.
2. Build `llama-server` when missing.
3. Download a tiny GGUF model to `/tmp/ghostlink-models/stories15M-q4_0.gguf` when missing.
4. Start `llama-server` on `127.0.0.1:8080`.
5. Start Ghostlink API on `127.0.0.1:8003` with native backend mode.

### Validation (real inference proof)

```bash
bash scripts/validate_native_llama_server.sh
```

Expected output includes:
- `real_inference=True`
- `inference_backend=native`

### Key environment variables

- `GHOSTLINK_MODEL_PATH`: override GGUF file path.
- `GHOSTLINK_MODEL_URL`: override download URL for first-time bootstrap.
- `GHOSTLINK_LLAMA_SERVER_HOST`: server bind host (default `127.0.0.1`).
- `GHOSTLINK_LLAMA_SERVER_PORT`: server port (default `8080`).
- `GHOSTLINK_API_HOST`: Ghostlink API host (default `127.0.0.1`).
- `GHOSTLINK_API_PORT`: Ghostlink API port (default `8003`).

## Docker

### Production compose

```bash
docker compose -f docker-compose.production.yml up --build
```

### Launch compose

```bash
docker compose -f docker-compose.launch.yml up --build
```

## Core Endpoints

- Health: `GET /health`
- Models: `GET /api/models`
- Model status: `GET /api/models/status`
- Metrics: `GET /api/metrics`
- Sessions: `GET /api/sessions`
- Workers: `GET /api/workers`
- Chat: `POST /api/inference/chat`

Example:

```bash
curl -s http://127.0.0.1:8003/health
curl -s http://127.0.0.1:8003/api/models
curl -s -X POST http://127.0.0.1:8003/api/inference/chat \
  -H 'Content-Type: application/json' \
  -d '{"message":"hello","max_tokens":32}'
```

## Capability Notes

- When Ollama is available and reachable, backend can route chat to real model inference.
- When Ollama is unavailable, backend returns deterministic fallback responses so the GUI and API remain usable.
- Tool/MCP payload fields are accepted by backend and reflected in response metadata; external MCP execution capability depends on configured servers and runtime integration.

## Validation Commands

Run these before opening a PR:

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
python3 scripts/verify_hf_models.py
```

Additional checks:

```bash
python3 scripts/test_api_contract.py
python3 scripts/test_gui_backend_integration.py
```

## Troubleshooting

### Port already in use

- Backend port: set `GHOSTLINK_HOST` / `GHOSTLINK_PORT`.
- GUI port: set `GUI_PORT`.

Example:

```bash
GHOSTLINK_PORT=8010 GUI_PORT=5178 bash launch-complete.sh
```

### GUI starts but backend not reachable

1. Check backend health:
   ```bash
   curl -s http://127.0.0.1:8003/health
   ```
2. Re-run backend manually:
   ```bash
   cargo run -p ghost-link -- serve 127.0.0.1 8003
   ```

### Ollama not installed

The stack still launches. Chat remains available with fallback behavior until Ollama is installed.

## Repository Pointers

- Backend crate: `crates/ghost-link`
- Core runtime/fabric: `crates/ghostlink-core`
- Modern GUI: `ghostlink_gui_modern`
- Launch orchestrator: `scripts/launch_studio.py`

