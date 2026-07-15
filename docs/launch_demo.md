# Ghostlink Demo Walkthrough

This guide walks through a complete Ghostlink Studio demo for prospects, investors, or new team members.

## Prerequisites

- Docker Desktop installed and running
- Git
- A modern web browser

## 1. Start the Demo Stack

```bash
docker compose -f docker-compose.demo.yml up --build
```

This starts three services:

| Service | Port | Purpose |
|---------|------|---------|
| model-manager | 8001 | Model management API |
| gateway | 9999 | Inference proxy |
| ollama | 11434 | Local LLM runtime |

Wait for all containers to report healthy status (30-60 seconds).

## 2. Verify Backend Health

```bash
curl -s http://localhost:8001/health
```

Expected: JSON response with status field.

## 3. Launch the Full Studio Stack

For the full Ghostlink Studio experience (GUI + native inference):

**Windows:**
```cmd
launch.bat
```

**Linux/macOS:**
```bash
bash launch.sh
```

This starts:
- llama-server on port 8080 (native GGUF inference)
- Ghostlink API on port 8003 (Rust backend)
- React GUI on port 5173 (Vite dev server)

The browser opens automatically to http://127.0.0.1:5173.

## 4. GUI Walkthrough

### Models Tab
1. Navigate to **Models** in the sidebar
2. Click **Refresh** to see available local models
3. Click **Load** on a model to activate it
4. Switch to **Hugging Face** tab to search and download new models

### Chat Tab
1. Navigate to **Chat** in the sidebar
2. Select the loaded model from the dropdown
3. Type a message and press Enter
4. Observe the real inference response

### Metrics Tab
1. Navigate to **Metrics** to see live throughput, latency (p50/p95), and resource utilization

### Workers Tab
1. Navigate to **Workers** to see cluster node status and load distribution

### Settings Tab
1. Navigate to **Settings** to configure inference parameters (temperature, top_k, GPU layers, etc.)

## 5. API Demonstration

Show the OpenAI-compatible API:

```bash
curl -X POST http://127.0.0.1:8003/api/inference/chat \
  -H "Content-Type: application/json" \
  -d '{"message": "Explain distributed inference", "max_tokens": 128}'
```

## 6. Key Talking Points

- **Distributed fabric**: Hardware-aware layer placement across CPU/GPU/NPU
- **Native inference**: Direct llama.cpp integration, no Ollama dependency required
- **OpenAI-compatible API**: Drop-in replacement for existing LLM toolchains
- **Self-hosted**: Full data sovereignty, no cloud dependencies
- **Zero-config clustering**: UDP discovery + automatic rebalancing

## 7. Tear Down

```bash
# Stop Docker demo stack
docker compose -f docker-compose.demo.yml down

# Stop Studio stack (press Ctrl+C in the launch terminal)
```
