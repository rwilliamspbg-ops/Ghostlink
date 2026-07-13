# Ghostlink

Ghostlink is a high-performance distributed inference fabric for teams building custom LLM systems. It combines hardware-aware planning, flexible routing, and model-management workflows so inference workloads can be distributed across heterogeneous devices with more explicit control than generic orchestration layers.

## Why Ghostlink
Ghostlink is designed for teams that want lower-latency planning, more control over distributed inference topologies, and a simpler path to custom LLM serving than generic orchestration stacks.

Use Ghostlink when you need:
- fast model and workload scheduling across heterogeneous hardware,
- a self-hosted inference fabric with open-source flexibility,
- a platform that can be extended into a paid commercial offering with support and enterprise deployment services.

## Public launch assets
A polished landing page and launch collateral are now available for the project:
- Live site: https://rwilliamspbg-ops.github.io/Ghostlink/
- Comparison sheet: [docs/comparison_sheet.md](docs/comparison_sheet.md)
- Demo flow: [docs/launch_demo.md](docs/launch_demo.md)

The main landing page highlights concrete demo themes such as adaptive model routing, hardware-aware placement, operational visibility, and a terminal-style request flow.

## Architecture at a glance
Ghostlink is organized around a small set of core layers:
- Runtime and planning: hardware detection, placement strategy, and inference scheduling logic.
- Control plane: model management, routing decisions, and service orchestration.
- Interfaces: CLI commands, an OpenAI-compatible API, and the web-based GUI experience.

This makes it easier to reason about the system as a distributed inference fabric rather than a single monolithic app.

## Project status
Ghostlink is currently positioned as a launch-ready open-source foundation with a strong demo story and public-facing collateral. The core project already supports local development workflows, model-management flows, and a browser-accessible landing experience.

Current strengths:
- a working local launch path for experimentation and demos,
- a clear positioning around distributed inference scheduling and routing,
- public assets for comparison, demo flow, and product storytelling.

Current focus areas:
- strengthening the end-to-end demo experience,
- improving documentation for deployment and production use,
- expanding real-world validation across more hardware and runtime setups.

## Contributing and roadmap
Contributions are welcome. A practical next step for contributors is to help improve the runtime experience, expand deployment guidance, and validate Ghostlink across more hardware combinations.

Near-term roadmap themes:
- improve end-to-end demo reliability and documentation,
- strengthen deployment and production guidance,
- expand validation for different runtimes and hardware profiles.

## FAQ
- Why use Ghostlink instead of a generic orchestrator? It focuses on latency-aware planning and custom inference topologies rather than acting as a broad-purpose scheduler.
- Does Ghostlink require specific hardware? No. It can run on CPU, GPU, NPU, and mixed setups, with detection and routing adapting to what is available.
- Can I use it for demos and early pilots? Yes. The project is designed to support local experiments, demos, and self-hosted evaluation before broader production rollout.

## Evaluation and contact
If you want to evaluate Ghostlink for a pilot, internal demo, or custom inference workflow, the easiest next step is to start from the public landing page and the demo flow documents. For deployment support, onboarding, or commercial discussions, use the repository as the initial point of contact and open a discussion or issue to begin the conversation.

## Quick Start (Windows)

### Prerequisites

| Tool | Required | How to Install |
|------|----------|---------------|
| **Rust** | Yes | `winget install Rustlang.Rustup` or https://rustup.rs |
| **Node.js** | Yes | `winget install OpenJS.NodeJS.LTS` or https://nodejs.org (LTS) |
| **CMake** | For llama.cpp | `winget install Kitware.CMake` or https://cmake.org/download/ |
| **Git** | For llama.cpp | `winget install Git.Git` |

### Launch

```powershell
# 1. Clone or open the Ghostlink directory
cd C:\Users\rwill\Ghostlink

# 2. Build the backend (one time)
cargo build --release -p ghost-link

# 3. Launch (cinematic splash + services)
.\launch.bat

# Or fast launch (skips build, uses existing binary)
.\launch-fast.bat
```

This starts three services:
- **llama-server** (inference engine, port 8080)
- **Ghostlink API** backend (port 8003)
- **React frontend** at http://127.0.0.1:5173

### Demo walkthrough
A simple product-style demo flow is:
1. Launch the local control plane and confirm the runtime is online.
2. Point Ghostlink at a local or remote model endpoint and review the route decision.
3. Submit a sample request and inspect the queue, placement, and status output.

### What to Expect

The splash screen shows:
- GPU/CPU/NPU hardware detected
- Component status (backend binary, llama-server, model)
- Service URLs once ready

Open http://127.0.0.1:5173 → **Models tab** → pick a model → **Chat tab** → start chatting.

## Quick Start (Linux / macOS)

```bash
# Prerequisites: Rust, Node.js, CMake, make
# Install Rust: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

cargo build --release -p ghost-link
./launch.sh
```

## Hardware Detection

Ghostlink auto-detects available accelerators at startup:

| Runtime | Detection |
|---------|-----------|
| **CUDA** (NVIDIA) | `nvidia-smi`, `CUDA_PATH` |
| **DirectML** (AMD iGPU, Intel ARC, any DX12 GPU) | WMI on Windows |
| **ROCm** (AMD discrete) | `rocm-smi`, `hipconfig` (requires `--features rocm`) |
| **Metal** (Apple Silicon) | `sysctl hw.optional.arm64` |
| **NPU** (AMD XDNA, Intel NPU, Qualcomm) | WMI on Windows, sysfs on Linux |
| **CPU** | Always available |

If your GPU isn't detected, set env vars manually:
```powershell
$env:GHOSTLINK_GPU_NAME="AMD Radeon 860M"
$env:GHOSTLINK_VRAM_GB=8
$env:GHOSTLINK_COMPUTE_CAPABILITY="gpu"
```

## Launch Scripts

| Script | Description |
|--------|-------------|
| `launch.bat` | Full cinematic launcher — builds llama.cpp, downloads model, starts all services |
| `launch-fast.bat` | Fast launcher — uses pre-built binary, skips cargo build |
| `launch-splash.bat` | Hardware detection splash + delegates to `launch-complete.bat` |
| `launch-complete.bat` | Starts backend, llama-server, and React GUI |
| `check_hardware.ps1` | Diagnostic — shows detected GPU, NPU, and component status |

## Usage (Developer)

### CLI Commands

```bash
# Build
cargo build --release -p ghost-link

# With AMD ROCm support
cargo build --release -p ghost-link --features rocm

# Generate a placement plan for your hardware
cargo run -p ghost-link -- plan

# Probe local hardware profile
cargo run -p ghost-link -- probe my-node
cargo run -p ghost-link -- probe my-node --full

# Start the OpenAI-compatible API server
cargo run -p ghost-link -- serve 127.0.0.1 8003

# Unified troubleshooting
cargo run -p ghost-link -- doctor --strict

# Run the full 30B planning flow
cargo run -p ghost-link -- flow iprada-16gb zenbook-32gb 32 32 64 4 tcp

# Launch the ASCII cluster dashboard
cargo run -p ghost-link -- dashboard
```

### API Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/health` | GET | Health check |
| `/api/models` | GET | List available models |
| `/api/models/status` | GET | Loaded model status |
| `/api/runtime/detect` | GET | Available runtimes (GPU, NPU, CPU) |
| `/api/runtime/models?runtime=directml` | GET | Models filtered by runtime |
| `/api/metrics` | GET | Performance metrics |
| `/api/inference/chat` | POST | Chat completion |

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `GHOSTLINK_INFERENCE_BACKEND` | `native` | `native` or `ollama` |
| `GHOSTLINK_NATIVE_ENGINE` | `llama_server` | `llama_server` or `llama_cpp` |
| `GHOSTLINK_LLAMA_SERVER_URL` | `http://127.0.0.1:8080/completion` | llama-server URL |
| `GHOSTLINK_GPU_NAME` | — | Override detected GPU name |
| `GHOSTLINK_VRAM_GB` | — | Override detected VRAM |
| `GHOSTLINK_COMPUTE_CAPABILITY` | — | Override compute capability |

### Config File (TOML)

See `ghostlink.toml` for all settings:
- Node identities and resource overrides
- Discovery broadcast configuration
- TCP transport tuning
- GUI Python path

## Architecture

```
crates/
├── ghostlink-core/     # Shared runtime primitives
│   ├── host.rs          # GPU/NPU/CPU auto-detection
│   ├── runtime.rs       # Pipeline execution (in-memory, TCP, AF_XDP)
│   ├── planning.rs      # Layer assignment & quantization
│   ├── discovery.rs     # UDP broadcast cluster discovery
│   ├── cluster.rs       # Thread-safe node state & metrics
│   ├── health.rs        # Network health & fault detection
│   └── load_balance.rs  # Tensor distribution across nodes
├── ghost-link/          # CLI demo & API server
ghostlink_gui_modern/    # React frontend (Vite + Tailwind)
```

## Testing

```bash
# Full test suite
cargo test --workspace

# With ROCm feature
cargo test --workspace --features rocm
```

## Comparison Snapshot
See [docs/comparison_sheet.md](docs/comparison_sheet.md) for a concise Ghostlink vs. vLLM / DeepSpeed / Ray / TensorRT-LLM positioning sheet.

## License

MIT
