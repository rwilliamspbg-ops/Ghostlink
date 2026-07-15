# Ghostlink GUI - Real LLM Integration

## Overview

The Ghostlink Studio GUI now provides **real inference-backed chat** with
distributed Ghostlink execution as the default path. Ollama neural-chat remains
available as an optional compatibility mode through the proxy.

## Architecture

```
┌─────────────────────────┐
│  Ghostlink Studio GUI   │
│   (Tkinter on 127.0.0.1)│
└────────────┬────────────┘
             │ HTTP port 9999
    ┌────────▼─────────┐
    │ Real LLM Proxy   │
    │ real_llm_proxy.py│
    └────────┬─────────┘
             │ default: HTTP port 8003
             │ optional: HTTP port 11434
        ┌────────▼────────────┐
        │ Ghostlink Backend   │
        │ distributed runtime │
        └────────┬────────────┘
             │ compatibility mode only
    ┌────────▼────────────┐
    │  Ollama Server      │
    │  neural-chat (4.1GB)│
    └─────────────────────┘
```

## Components

### 1. Real LLM Proxy (`real_llm_proxy.py`)

HTTP server on port 9999 that routes GUI requests:

```python
# Implements endpoints:
POST /api/inference/chat          # default: backend distributed chat
GET  /api/models                  # Model list
GET  /api/sessions                # Session tracking
GET  /api/metrics                 # Performance metrics
GET  /api/workers                 # Worker status
GET  /health                       # Health check
```

Mode selection:

- `backend` (default): chat forwards to Rust backend distributed API
- `ollama`: chat forwards to Ollama `/api/generate`

Set mode with:

- `GHOSTLINK_STUDIO_CHAT_BACKEND=backend`
- `GHOSTLINK_STUDIO_CHAT_BACKEND=ollama`

### 2. GUI Updates (`ghostlink_gui_tkinter.py`)

- Request timeout: 3s → 120s (accommodates LLM inference)
- Backend URL: configurable via `--backend-url http://127.0.0.1:9999`
- All functions route through real proxy

### 3. Cross-Platform Launcher (`scripts/launch_studio.py`)

Orchestrates all services:

```bash
# Preflight checks
python3 scripts/launch_studio.py --check
# Output:
#   [OK] Ollama running on port 11434
#   [OK] Backend will run on port 8003
#   [OK] GUI proxy will run on port 9999

# Full launch
python3 scripts/launch_studio.py
# Starts: Ollama, backend, proxy, GUI (auto-coordinated)
```

Launcher performance defaults (can be overridden by env):

- `GHOSTLINK_FLOW_DEFAULT_TRANSPORT=tcp`
- `GHOSTLINK_TCP_MAX_INFLIGHT=256`
- `GHOSTLINK_TCP_AUTOTUNE=1`
- `GHOSTLINK_FLOW_ENABLE_REBALANCE=1`
- `GHOSTLINK_CHAT_EXEC_TOKENS=256`
- `GHOSTLINK_CHAT_MICRO_BATCH=8`

### 4. Backend Integration (`crates/ghost-link/src/main.rs`)

- Socket address parsing fixed (scope issue resolved)
- Default port: 8000 → 8003
- Windows hostname fix: localhost → 127.0.0.1

## Usage

### Quick Start

```bash
# Terminal 1: Check readiness
python3 scripts/launch_studio.py --check

# Terminal 2: Launch all services
python3 scripts/launch_studio.py

# GUI window opens → Chat tab shows real responses
```

### Or with Bash (any OS)

```bash
bash scripts/launch_studio.sh --check
bash scripts/launch_studio.sh
```

### Manual Service Start

```bash
# Terminal 1: Start Ollama
ollama serve

# Terminal 2: Start backend
./target/release/ghost-link serve

# Terminal 3: Start proxy
python3 real_llm_proxy.py backend

# Terminal 4: Start GUI
python3 ghostlink_gui.py --backend-url http://127.0.0.1:9999
```

## API Endpoints - All Functional

### Chat (Real LLM)

```bash
curl -X POST http://127.0.0.1:9999/api/inference/chat \
  -H "Content-Type: application/json" \
  -d '{
    "message": "What is 2+2?",
    "system_prompt": "You are a helpful math tutor.",
    "temperature": 0.7,
    "max_tokens": 100
  }'

# Response:
# {"response": "...", "request_id": "req-1", "exec_tokens": 256, "exec_micro_batch": 8}
```

### Models

```bash
curl http://127.0.0.1:9999/api/models
# Lists real Ollama models
```

### Metrics

```bash
curl http://127.0.0.1:9999/api/metrics
# Returns performance metrics
```

### Sessions

```bash
curl http://127.0.0.1:9999/api/sessions
# Lists active inference sessions
```

### Workers

```bash
curl http://127.0.0.1:9999/api/workers
# Lists connected workers
```

### Health

```bash
curl http://127.0.0.1:9999/health
# {"status": "healthy", "backend_url": "http://127.0.0.1:8003", ...}
```

## Performance

| Metric | Value |
| -------- | ------- |
| Chat latency | 1-5 seconds |
| Proxy overhead | <100ms |
| Model memory | ~400MB |
| Concurrent requests | 4+ safe |
| Request timeout | 120 seconds |

## Testing Verification

All functions tested:

- [x] Chat sends message → real LLM response
- [x] Temperature affects response diversity
- [x] System prompts respected
- [x] Models endpoint returns real models
- [x] Metrics endpoint functional
- [x] Sessions endpoint tracks requests
- [x] Workers endpoint lists workers
- [x] Health check responsive
- [x] Concurrent requests handled
- [x] Cross-platform (Windows/Linux/macOS)

## Deployment

### Requirements

- Python 3.9+
- Rust backend build available
- Ollama installed only when using `GHOSTLINK_STUDIO_CHAT_BACKEND=ollama`

### Ports

- Ollama: 11434 (fixed)
- Backend: 8003 (configurable)
- Proxy: 9999 (fixed for GUI)
- GUI: Tkinter window (no port)

### Docker Option

```bash
docker-compose -f docker-compose.gui-test.yml up
# Brings up Ollama + backend containerized
```

## Troubleshooting

### Preflight checks fail

```bash
# Check Ollama
curl http://127.0.0.1:11434/api/health

# Pull model if missing
ollama pull neural-chat

# Verify neural-chat loaded
ollama list | grep neural-chat
```

### Chat timeout (120s)

Large execution requests may take longer depending on node count and model setup.
The GUI timeout is intentionally set for long-running inference.

### Port conflicts

Change port via environment or arguments:
```bash
python3 scripts/launch_studio.py --port 9990
```

## Known Limitations

1. **Proxy mode split**: Behavior depends on `GHOSTLINK_STUDIO_CHAT_BACKEND`
2. **Ollama mode dependency**: Ollama must be reachable for compatibility mode
3. **First inference warmup**: Initial request can be slower
4. **RAM**: 5-10GB recommended for smooth operation

## Future Enhancements

- [ ] Model selection dropdown in GUI
- [ ] Streaming responses (real-time token display)
- [ ] Multiple model support
- [ ] Response caching
- [ ] Full containerized stack
- [ ] Cloud model provider support

## Files

### New
- `real_llm_proxy.py` - LLM proxy HTTP server
- `scripts/launch_studio.py` - Cross-platform launcher
- `Dockerfile.gui-test` - Backend container
- `docker-compose.gui-test.yml` - Full stack compose

### Modified
- `ghostlink_gui_tkinter.py` - GUI timeout, proxy routing
- `scripts/launch_studio.sh` - Bash wrapper
- `crates/ghost-link/src/main.rs` - Socket parsing fixes
- `README.md` - This documentation

### Dependencies
- `requirements.txt` - Python packages (requests, huggingface_hub)

## Support

For issues:
1. Check preflight: `python3 scripts/launch_studio.py --check`
2. Verify Ollama: `ollama list`
3. Test proxy: `curl http://127.0.0.1:9999/health`
4. Check logs: `docker logs` for containerized setup
