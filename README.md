# Ghostlink

A high-performance distributed LLM inference fabric that turns spare local GPUs into a shared execution surface. Ghostlink provides zero-config discovery, hardware-aware pipeline planning, and multi-transport execution across heterogeneous devices.

## Architecture

Ghostlink is organized as a Rust workspace with two crates:

```
crates/
├── ghostlink-core/     # Shared runtime primitives
│   ├── runtime.rs      # Pipeline execution (in-memory, TCP, AF_XDP)
│   ├── host.rs          # GPU/Runtime auto-detection
│   ├── ring.rs          # Zero-copy SPSC ring buffer
│   ├── protocol.rs      # Binary protocol with CRC32 integrity
│   ├── cluster.rs       # Thread-safe node state & metrics
│   ├── planning.rs      # Layer assignment & quantization
│   ├── health.rs        # Network health & fault detection
│   ├── load_balance.rs  # Tensor distribution across nodes
│   ├── accelerator.rs   # CPU SIMD paths (AVX2, AVX-512, NEON)
│   └── discovery.rs     # UDP broadcast discovery
├── ghost-link/          # CLI demo & API server
```

**Control plane** (`control-plane/`) is a lightweight Go service registry for multi-host deployments.

## Runtime Detection

Ghostlink detects and profiles available hardware on startup:

| Runtime | Detection Method | Feature Flag |
|---------|-----------------|--------------|
| CUDA (NVIDIA) | `nvidia-smi`, `CUDA_PATH` | Always enabled |
| ROCm (AMD) | `rocm-smi`, `hipconfig`, WMI, sysfs | `--features rocm` |
| Metal (Apple) | `sysctl hw.optional.arm64` | Always enabled (macOS) |
| NPU | Env vars, sysfs indicators | Always enabled |
| CPU | Always available | Always enabled |

The probe chain is: `nvidia-smi → rocm-smi → WMI (AMD) → lspci → sysfs → env vars`. When the `rocm` feature is disabled, AMD GPUs are detected via lspci/sysfs with generic `"gpu"` compute capability. When enabled, they are identified with `"rocm"` capability and routed through AMD-specific probe paths.

### Device Types

Pipelines can target four device kinds with calibrated cost models:

| DeviceKind | Per-Layer Cost | Use Case |
|------------|---------------|----------|
| `Npu` | 0.42 ms | Neural processors (Qualcomm, MediaTek) |
| `Gpu` | 0.55 ms | NVIDIA CUDA GPUs |
| `RocmGpu` | 0.58 ms | AMD GPUs via ROCm/HIP |
| `Cpu` | 1.25 ms | CPU fallback (AVX2/AVX-512/NEON) |

## Pipeline Execution

Ghostlink supports three transport modes for inter-stage communication:

- **In-Memory**: Channel-backed zero-copy SPSC ring buffers for single-process execution
- **TCP Loopback**: Socket-backed transport with HMAC auth, configurable inflight, reconnect with backoff, and autotune
- **AF_XDP**: Kernel-bypass transport on Linux with automatic TCP fallback

Transport autotuning sweeps max-inflight candidates and caches optimal values to `tmp/tcp_autotune_cache.tsv`.

### Execution Flow

```
Input Tokens → [Stage 0] → Bridge → [Stage 1] → Bridge → ... → [Stage N] → Output
                  ↑                          ↑
              DeviceKind                Transport Mode
              (Npu/Gpu/RocmGpu/Cpu)    (InMem/TCP/XDP)
```

Each stage runs a bounded transform on batched token payloads. Bridges handle serialization, HMAC authentication, and reconnection. Cluster health metrics are fed back into the planner for dynamic rebalancing.

## Usage

### CLI Commands

```bash
# Build
cargo build --release -p ghost-link

# With AMD ROCm support
cargo build --release -p ghost-link --features rocm

# Run tests
cargo test --workspace
cargo test --workspace --features rocm

# Generate a placement plan for your hardware
cargo run -p ghost-link -- plan

# Probe local hardware profile
cargo run -p ghost-link -- probe my-node
cargo run -p ghost-link -- probe my-node --full

# Join a cluster node
cargo run -p ghost-link -- join node-02

# Listen for discovery broadcasts
cargo run -p ghost-link -- listen workstation-a --once

# Run the full 30B planning flow with TCP loopback
cargo run -p ghost-link -- flow iprada-16gb zenbook-32gb 32 32 64 4 tcp

# Run with in-memory transport
cargo run -p ghost-link -- flow iprada-16gb zenbook-32gb 32 32 64 4 inmem

# Start the OpenAI-compatible API server
cargo run -p ghost-link -- serve 127.0.0.1 8003

# Launch the ASCII cluster dashboard
cargo run -p ghost-link -- dashboard

# Unified troubleshooting
cargo run -p ghost-link -- doctor --strict
cargo run -p ghost-link -- doctor --strict --json ./tmp/doctor-report.json
cargo run -p ghost-link -- doctor --network-probe

# Start a local cluster for testing
cargo run -p ghost-link -- cluster-start 3 46000
```

### API Endpoints (when serving)

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/health` | GET | Health check |
| `/api/models` | GET | List available models |
| `/api/models/status` | GET | Loaded model status |
| `/api/models/by-runtime?runtime=rocm` | GET | Models filtered by runtime |
| `/api/metrics` | GET | Performance metrics |
| `/api/sessions` | GET | Chat sessions |
| `/api/workers` | GET | Worker status |
| `/api/inference/chat` | POST | Chat completion |

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `GHOSTLINK_INFERENCE_BACKEND` | `ollama` | `ollama` or `native` |
| `GHOSTLINK_CONFIG` | `./ghostlink.toml` | Config file path |
| `GHOSTLINK_DISCOVERY_LISTEN` | `0.0.0.0:45885` | Discovery bind address |
| `GHOSTLINK_DISCOVERY_BROADCAST` | `255.255.255.255:45885` | Discovery broadcast target |
| `GHOSTLINK_TCP_MAX_INFLIGHT` | `256` | Max inflight batches per bridge |
| `GHOSTLINK_TCP_AUTOTUNE` | `1` | Enable transport autotuning |
| `GHOSTLINK_TCP_AUTH_TOKEN` | — | HMAC auth token for bridges |
| `GHOSTLINK_XDP_INTERFACE` | `eth0` | AF_XDP interface name |
| `GHOSTLINK_FLOW_ENABLE_REBALANCE` | `0` | Enable runtime rebalancing |
| `GHOSTLINK_GPU_NAME` | — | Override detected GPU name |
| `GHOSTLINK_VRAM_GB` | — | Override detected VRAM |
| `GHOSTLINK_COMPUTE_CAPABILITY` | — | Override compute capability |
| `GHOSTLINK_SYSTEM_MEMORY_GB` | — | Override system memory |

### Config File

Ghostlink supports TOML config files with per-section defaults:

```toml
[flow]
local_id = "iprada-16gb"
remote_id = "zenbook-32gb"
remote_vram_gb = 32.0
remote_system_memory_gb = 32.0
execution_tokens = 64
micro_batch = 4
transport = "tcp"

[discovery]
listen = "0.0.0.0:45885"
broadcast = "255.255.255.255:45885"
timeout_ms = 2000
auth_token = "my-secret"

[tcp]
max_inflight = 256
reconnect_attempts = 3
reconnect_backoff_ms = 25
auth_token = "my-secret"

[cluster_start]
node_count = 4
base_port = 46000

[gui]
python = "/usr/bin/python3.11"
```

## Performance

The pipeline execution engine records per-stage and aggregate metrics:

```
Execution Runtime
=================
Tokens: 64 | Micro-batch: 4 | Batches: 16 | Stages: 2
Measured wall-clock time: 12.34 ms
Throughput: 5187.20 tokens/sec
Avg token latency: 0.77 ms | P95: 1.23 ms
```

Transport autotuning selects optimal inflight depth via candidate sweep, improving throughput by reducing bridge contention. Results are cached to `tmp/tcp_autotune_cache.tsv`.

## Testing

```bash
# Full test suite
cargo test --workspace

# With ROCm feature
cargo test --workspace --features rocm

# Integration tests only
cargo test --test integration
cargo test --test multinode_heterogeneity_and_npu_perf

# Run benchmarks
cargo bench

# Lint
cargo clippy --workspace --all-targets -- -D warnings
```

Test coverage includes:
- Runtime detection (CUDA, ROCm, Metal, NPU, CPU)
- Device mapping and pipeline planning
- Multi-node discovery and registration
- Ring buffer stress tests (concurrent, wrap-around, rate mismatch)
- Protocol encoding/decoding with CRC corruption detection
- Network failure injection and recovery
- Health monitoring and adaptive quantization
- AF_XDP scaffolding validation
- Heterogeneous device pipelines (NPU + GPU + CPU + ROCm)

## Requirements

- **Rust**: 1.85.0+ (MSRV)
- **Optional**: CMake 3.20+ (for native llama.cpp inference)
- **Optional**: Node.js 18+ (for React GUI)
- **Optional**: Go 1.21+ (for control-plane)

## License

MIT
