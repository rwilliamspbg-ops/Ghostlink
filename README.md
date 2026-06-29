# Ghostlink Studio

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
- **Ultra-Low Overhead**: Zero-copy SPSC ring buffers with backpressure handling for maximum throughput.
- **Heterogeneous Scaling**: Seamlessly combine NPU, GPU, and CPU resources across your local network.
- **Enterprise Security**: HMAC-SHA256 authenticated discovery and optional mTLS for secure inter-node communication.
- **Adaptive Quantization**: Runtime-aware planning that adjusts to network quality and hardware capability.

## 📊 Performance Benchmarks

Measured in a standard development environment:

| Transport Mode | Avg Throughput (tokens/s) | Avg P95 Latency (ms) |
| :--- | :---: | :---: |
| **In-Memory** | **118,840.29** | **1.83** |
| **TCP Loopback** | **67,794.12** | **3.65** |

*Benchmarks conducted on 2026-06-29 using Mistral-7B baseline.*

## 🛠 Installation & Usage

### Prerequisites
- Stable Rust (1.75+)
- Python 3.10+

### Manual Setup
```bash
# Build the high-performance core
cargo build --release --workspace

# Launch the Studio GUI
cargo run -p ghost-link -- gui
```

## 📚 Documentation

- [Quickstart Guide](docs/QUICKSTART.md)
- [Architecture Overview](docs/ARCHITECTURE.md)
- [Deployment Guide](docs/DEPLOYMENT.md)
- [Security Model](docs/SECURITY_MODEL.md)

## ⚖️ License

Ghost-Link is released under the MIT License.
