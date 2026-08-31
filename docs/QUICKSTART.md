# Quickstart

## One-command launch (Ollama)

**Windows:**
```powershell
launch-ollama.bat
```

The script:
1. Starts Ollama server on `http://127.0.0.1:11434`
2. Builds and starts the Rust backend API on `http://127.0.0.1:8000 (gateway; :8003 internal)`
3. Installs npm dependencies
4. Starts Vite dev server on `http://127.0.0.1:5173`

## Prerequisites

- **Ollama** — Download from https://ollama.com
- **Rust** — `winget install Rust.Rustup` or https://rustup.rs
- **Node.js** — `winget install OpenJS.NodeJS` or https://nodejs.org

## What you can do

1. **Chat** — Select a model, send messages with SSE streaming
2. **Download models** — Pull models from Ollama library or browse Hugging Face
3. **Monitor metrics** — CPU/GPU/memory utilization, active sessions
4. **Cluster workers** — Add remote workers, discover peers on LAN
5. **Settings** — Configure ports, backend selection, auth tokens

## Pull models

```powershell
ollama pull llama3.2:3b
ollama pull gemma2:2b
ollama pull qwen2.5:4b
```

Or use the "Popular Ollama Models" grid in the GUI's Models tab.

## Environment shortcuts

```powershell
set GHOSTLINK_INFERENCE_BACKEND=native  # Use legacy native backend instead of Ollama
```
