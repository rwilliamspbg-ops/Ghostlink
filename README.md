# Ghostlink Studio

[![CI](https://github.com/rwilliamspbg-ops/Ghostlink/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/rwilliamspbg-ops/Ghostlink/actions/workflows/ci.yml)
[![Production Gate](https://github.com/rwilliamspbg-ops/Ghostlink/actions/workflows/production-gate.yml/badge.svg?branch=main)](https://github.com/rwilliamspbg-ops/Ghostlink/actions/workflows/production-gate.yml)
[![Tests](https://github.com/rwilliamspbg-ops/Ghostlink/actions/workflows/tests.yml/badge.svg?branch=main)](https://github.com/rwilliamspbg-ops/Ghostlink/actions/workflows/tests.yml)
[![Lint](https://github.com/rwilliamspbg-ops/Ghostlink/actions/workflows/lint.yml/badge.svg?branch=main)](https://github.com/rwilliamspbg-ops/Ghostlink/actions/workflows/lint.yml)
[![Security](https://github.com/rwilliamspbg-ops/Ghostlink/actions/workflows/security.yml/badge.svg?branch=main)](https://github.com/rwilliamspbg-ops/Ghostlink/actions/workflows/security.yml)
[![Docs](https://github.com/rwilliamspbg-ops/Ghostlink/actions/workflows/docs.yml/badge.svg?branch=main)](https://github.com/rwilliamspbg-ops/Ghostlink/actions/workflows/docs.yml)
[![Markdown Lint](https://github.com/rwilliamspbg-ops/Ghostlink/actions/workflows/markdownlint.yml/badge.svg?branch=main)](https://github.com/rwilliamspbg-ops/Ghostlink/actions/workflows/markdownlint.yml)
[![Benchmarks](https://github.com/rwilliamspbg-ops/Ghostlink/actions/workflows/benchmarks.yml/badge.svg?branch=main)](https://github.com/rwilliamspbg-ops/Ghostlink/actions/workflows/benchmarks.yml)
[![HF Model Verify](https://github.com/rwilliamspbg-ops/Ghostlink/actions/workflows/hf-model-verify.yml/badge.svg)](https://github.com/rwilliamspbg-ops/Ghostlink/actions/workflows/hf-model-verify.yml)
[![Release Artifacts](https://github.com/rwilliamspbg-ops/Ghostlink/actions/workflows/release-artifacts.yml/badge.svg)](https://github.com/rwilliamspbg-ops/Ghostlink/actions/workflows/release-artifacts.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/Rust-stable-orange.svg)](https://www.rust-lang.org)
[![Python](https://img.shields.io/badge/Python-3.10%2B-blue.svg)](https://www.python.org)

Ghostlink is a high-performance LAN fabric designed to turn spare local GPUs and CPU hosts into a unified execution surface for large-model inference. By leveraging zero-copy SPSC primitives and host-aware autotuning, Ghostlink provides a professional-grade environment for running and testing massive models on heterogeneous hardware.

## 🚀 One-Click Launch

Get Ghostlink Studio up and running in seconds:

```bash
bash scripts/launch_studio.sh
```

This script automates environment setup, builds the high-performance core, and launches the Ghostlink Studio GUI.

## 💎 Professional Features

- **Professional Studio GUI**: A modern, dark-themed interface for model management, chat, and cluster analytics.
- **OpenAI-Compatible API**: Standard `/v1/chat/completions` endpoint for easy integration with existing LLM tools.
- **Ultra-Low Overhead**: Zero-copy SPSC ring buffers with backpressure handling for maximum throughput.
- **Heterogeneous Scaling**: Seamlessly combine NPU, GPU, and CPU resources across your local network.
- **Enterprise Security**: HMAC-SHA256 authenticated discovery and transport for secure inter-node communication.
- **Adaptive Quantization**: Runtime-aware planning that adjusts to network quality and hardware capability.

## 📊 Performance Benchmarks

Measured in a standard development environment:

| Transport Mode | Avg Throughput (tokens/s) | Avg P95 Latency (ms) |
| :--- | :---: | :---: |
| **In-Memory (Zero-Copy)** | **235,563.63** | **1.26** |
| **TCP Loopback (Optimized)** | **96,917.48** | **3.25** |

*Benchmarks from local gate-profile runs on 2026-07-01 (`flow` with 256 tokens, micro-batch 4).* 

### Performance Maturity Scorecard

Use the maturity profiler to rank optimization priority by stability, tail risk,
baseline headroom, and noise index from each snapshot run:

```bash
# Deterministic profile scorecard
python3 scripts/perf_maturity_profile.py \
  --summary tmp/perf_maturity_det/summary.json \
  --baseline docs/PERF_BASELINE.json \
  --stage-glob-template 'tmp/perf_maturity_det/{mode}-*.json' \
  --output-json tmp/perf_maturity_det/maturity_scorecard.json

# Stress profile scorecard
python3 scripts/perf_maturity_profile.py \
  --summary tmp/perf_maturity_stress/summary.json \
  --baseline docs/PERF_BASELINE_STRESS.json \
  --stage-glob-template 'tmp/perf_maturity_stress/{mode}-*.json' \
  --output-json tmp/perf_maturity_stress/maturity_scorecard.json
```

Recommended publication pattern:

- Attach the markdown scorecard table to the README release/perf section.
- Keep raw run files in CI artifacts or `tmp/` outputs, not inline in README.
- Treat classes as action levels: `optimize-now`, `next-batch`, `likely-noise`.

## 🛠 Command Line Interface

The `ghost-link` CLI provides powerful primitives for cluster management and performance profiling.

### Core Commands
- `gui` - Launch the Ghostlink Studio desktop interface.
- `serve` - Start the OpenAI-compatible API server.
- `join [id]` - Broadcast discovery frames to join a local cluster.
- `listen [id]` - Listen for and respond to discovery requests from peers.
- `flow` - Run a full 30B model planning and execution flow over real runtime transport (`tcp` or `inmem`).
- `doctor` - Run unified troubleshooting checks for environment and network.
- `dashboard` - Display the live ASCII cluster status dashboard.
- `cluster-start` - Spin up a multi-node local cluster for validation.

### Profiling & Discovery
- `probe [id]` - Detect local compute capabilities and recommended worker counts.
- `plan` - Generate a greedy layer placement plan across the current cluster.

## ⚙️ Configuration

Ghostlink can be configured via a `ghostlink.toml` file or environment variables.

Doctor JSON checks now include an optional `context` object for machine-readable fields.
For example, `network-probe` can emit `target`, `resolved`, `reachable`, `latency_ms`, and `timeout_ms`.
When `GHOSTLINK_PYTHON` is unset, GUI and doctor commands prefer the repository `.venv/bin/python`
before falling back to `python3`.

Optionally test connectivity to a target endpoint:

```bash
# Override the GUI/doctor Python interpreter only when you need something other than the repo .venv
GHOSTLINK_PYTHON=/usr/bin/python3 cargo run -p ghost-link -- gui-check --strict

# Include optional lightweight connectivity probe
cargo run -p ghost-link -- doctor --network-probe --network-target 127.0.0.1:8003
```

### Environment Variables

| Variable | Description | Default |
| :--- | :--- | :--- |
| `GHOSTLINK_CONFIG` | Path to TOML configuration file | `./ghostlink.toml` |
| `GHOSTLINK_TCP_AUTH_TOKEN` | Shared secret for transport authentication | - |
| `GHOSTLINK_DISCOVERY_AUTH_TOKEN` | Shared secret for UDP discovery authentication | - |
| `GHOSTLINK_TCP_MAX_INFLIGHT` | Max concurrent batches in TCP bridge | `512` |
| `GHOSTLINK_PYTHON` | Path override for GUI/doctor Python executable (when unset, prefers repo `.venv/bin/python` then `python3`) | `repo .venv/bin/python` if present, else `python3` |
| `GHOSTLINK_DISTRIBUTED_SMOKE` | Enable distributed runtime validation in `flow` | `false` |

## ⚠️ Runtime Notes

- `crates/ghostlink-core/src/xdp.rs` is currently experimental scaffolding and does not provide a working AF_XDP data path in this build.
- Discovery authentication is HMAC-SHA256 by default. Enabling `GHOSTLINK_DISCOVERY_ALLOW_LEGACY_CRC32` switches discovery fallback parsing to a compatibility checksum mode that is not cryptographic authentication.

## 📚 Documentation

- [Quickstart Guide](docs/QUICKSTART.md) - Fastest path to a running system.
- [Architecture Overview](docs/ARCHITECTURE.md) - Deep dive into zero-copy primitives.
- [Deployment Guide](docs/DEPLOYMENT.md) - Strategies for multi-node LAN setups.
- [Security Model](docs/SECURITY_MODEL.md) - Details on HMAC and authentication.

## ⚖️ License

Ghost-Link is released under the MIT License.

## 🧪 Development & Validation

Ghostlink maintains high quality through automated testing and validation gates:

- **Workspace Tests**: `cargo test --workspace`
- **Full Local Validation Bundle**: `bash scripts/setup_full_test_env.sh && bash scripts/run_full_validation.sh`
- **Linting**: `cargo clippy --workspace --all-targets -- -D warnings`
- **Model Verification**: `python3 scripts/verify_hf_models.py`

GitHub Actions gate workflows:

- **CI**: `.github/workflows/ci.yml`
- **Production Gate**: `.github/workflows/production-gate.yml`
- **Tests**: `.github/workflows/tests.yml`
- **Lint**: `.github/workflows/lint.yml`
- **Security**: `.github/workflows/security.yml`
- **Docs**: `.github/workflows/docs.yml`
- **Markdown Lint**: `.github/workflows/markdownlint.yml`
- **Benchmarks**: `.github/workflows/benchmarks.yml`
- **HF Model Verify**: `.github/workflows/hf-model-verify.yml`
- **Release Artifacts**: `.github/workflows/release-artifacts.yml`
