# Ghostlink

[![GitHub Repo](https://img.shields.io/badge/GitHub-Ghostlink-181717?logo=github)](https://github.com/rwilliamspbg-ops/Ghostlink)
[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](https://github.com/rwilliamspbg-ops/Ghostlink/blob/main/LICENSE)
[![Status](https://img.shields.io/badge/Status-Launch%20ready-22c55e)](https://rwilliamspbg-ops.github.io/Ghostlink/)

Distributed inference fabric for custom LLM systems. Routes workloads across CPU, GPU, and NPU resources with hardware-aware placement.

## Quick Start

**Windows:**
```powershell
launch-complete.bat
```

**Linux/macOS:**
```bash
bash launch-complete.sh
```

The script auto-detects hardware, downloads a small default model, starts the backend API + frontend dev server, and opens the GUI at `http://127.0.0.1:5173`.

## Launch Scripts

| Script | Purpose |
|---|---|
| `launch-complete.bat` / `.sh` | Full stack: hardware detection, defaults, dev server |
| `launch-fast.bat` / `.sh` | Same but skips build (set `GHOSTLINK_SKIP_BUILD=1`) |
| `launch.bat` / `.sh` | Cinematic wrapper around `launch-complete` |

Set `GHOSTLINK_SKIP_MODEL=1` to skip the default model download.

## Architecture

- **Backend** (`crates/ghost-link`): Rust/axum API server — model management, inference proxy, cluster discovery, audit logging
- **Frontend** (`ghostlink_gui_modern/`): React + Vite + TypeScript — chat, models, metrics, sessions, workers, settings
- **Inference**: llama-server (GGUF) or ollama backend, selected via `GHOSTLINK_INFERENCE_BACKEND`

Default ports: API `8003`, GUI `5173`, llama-server `8080`.

## API Endpoints

| Endpoint | Method | Description |
|---|---|---|
| `/api/models` | GET | List models |
| `/api/models/download` | POST | Download model from HuggingFace |
| `/api/models/download/progress` | GET | Poll download progress |
| `/api/models/load` | POST | Load model into inference engine |
| `/api/models/:name/unload` | POST | Unload model |
| `/api/models/search/huggingface` | GET | Search HF for GGUF models |
| `/api/inference/chat` | POST | Chat completion (SSE streaming) |
| `/api/sessions` | GET | Active inference sessions |
| `/api/metrics` | GET | System metrics |
| `/api/workers` | GET | Cluster workers |
| `/api/settings` | GET/POST | Runtime settings |
| `/api/runtime/detect` | GET | Detect available runtimes (CUDA/Metal/DirectML/NPU/CPU) |
| `/api/security/audit-log` | GET | Audit trail |
| `/api/security/pqc-state` | GET | PQC feature state |
| `/health` | GET | Backend health |

## Environment Variables

| Variable | Default | Description |
|---|---|---|
| `BACKEND_HOST` | `127.0.0.1` | API bind address |
| `BACKEND_PORT` | `8003` | API port |
| `GUI_PORT` | `5173` | Vite dev server port |
| `GHOSTLINK_INFERENCE_BACKEND` | `native` | `native` or `ollama` |
| `GHOSTLINK_SKIP_MODEL` | `0` | Skip default model download |
| `GHOSTLINK_SKIP_BUILD` | `0` | Skip Rust build step |
| `GHOSTLINK_INSECURE_TLS` | — | Skip TLS cert validation (for HF downloads behind proxies) |
| `GHOSTLINK_DISCOVERY_TIMEOUT_MS` | `3000` | Peer discovery timeout |

## Documentation

- [Architecture](docs/ARCHITECTURE.md)
- [Quickstart](docs/QUICKSTART.md)
- [Troubleshooting](docs/TROUBLESHOOTING.md)
- [Deployment](docs/DEPLOYMENT.md)
- [Security Model](docs/SECURITY_MODEL.md)
- [Benchmarks](docs/BENCHMARKS.md)

## License

MIT
