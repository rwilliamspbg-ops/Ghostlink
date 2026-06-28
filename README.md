# Ghost-Link

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![CI](https://github.com/rwilliamspbg-ops/Ghostlink/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/rwilliamspbg-ops/Ghostlink/actions/workflows/ci.yml)
[![Docs](https://img.shields.io/github/last-commit/rwilliamspbg-ops/Ghostlink/docs/runtime-probe-docs/.github/workflows/docs.yml?label=Docs%20workflow)](https://github.com/rwilliamspbg-ops/Ghostlink/actions/workflows/docs.yml)
[![Lint](https://img.shields.io/github/last-commit/rwilliamspbg-ops/Ghostlink/docs/runtime-probe-docs/.github/workflows/lint.yml?label=Lint%20workflow)](https://github.com/rwilliamspbg-ops/Ghostlink/actions/workflows/lint.yml)
[![Tests](https://img.shields.io/github/last-commit/rwilliamspbg-ops/Ghostlink/docs/runtime-probe-docs/.github/workflows/tests.yml?label=Tests%20workflow)](https://github.com/rwilliamspbg-ops/Ghostlink/actions/workflows/tests.yml)
[![HF Model Verify](https://img.shields.io/github/last-commit/rwilliamspbg-ops/Ghostlink/docs/runtime-probe-docs/.github/workflows/hf-model-verify.yml?label=HF%20Model%20Verify%20workflow)](https://github.com/rwilliamspbg-ops/Ghostlink/actions/workflows/hf-model-verify.yml)
[![Benchmarks](https://github.com/rwilliamspbg-ops/Ghostlink/actions/workflows/benchmarks.yml/badge.svg?branch=main)](https://github.com/rwilliamspbg-ops/Ghostlink/actions/workflows/benchmarks.yml)
[![Rust](https://img.shields.io/badge/Rust-stable-orange.svg)](https://www.rust-lang.org)
[![Coverage](https://img.shields.io/badge/Coverage-not%20measured-lightgrey)](TESTING.md)

Ghost-Link is an open-source LAN fabric for turning spare local GPUs and CPU hosts into a shared execution surface for large-model inference and training. The project focuses on low-overhead runtime primitives, binary discovery, host-aware autotuning, and runtime-selected execution backends rather than heavy orchestration.

## Features

- Zero-copy SPSC ring buffers with backpressure handling
- Binary Layer-2 discovery protocol with CRC32 validation
- Thread-safe cluster state with live metrics and fault tracking
- Runtime-aware planning, load balancing, and health thresholds
- Fast and full hardware probe modes with cached host detection
- Runtime-selected execution backends for GPU, AVX-512, AVX2, NEON, and generic CPU paths
- Terminal dashboard and CLI demo commands

## Install

```bash
# Install stable Rust
curl https://sh.rustup.rs -sSf | sh -s -- -y
. "$HOME/.cargo/env"

# Clone the repository
git clone https://github.com/rwilliamspbg-ops/Ghostlink.git
cd Ghostlink

# Build the workspace
cargo build --workspace
```

## Usage

```bash
# Run the full workspace test suite
cargo test --workspace

# Run the package-owned integration suite
cargo test -p ghostlink-core --test integration

# Generate a layer placement plan
cargo run -p ghost-link -- plan

# Emit a join frame for a specific node ID
cargo run -p ghost-link -- join node-02

# Start a UDP discovery responder (service loop)
cargo run -p ghost-link -- listen local-node

# Render the sample dashboard
cargo run -p ghost-link -- dashboard

# Launch vendored Mohawk GUI (requires Python + PyQt6 deps)
python3 -m pip install -r third_party/mohawk_gui/requirements.txt
# Linux containers may also require: sudo apt-get install -y libgl1
cargo run -p ghost-link -- gui --host localhost --port 8003

# Detect the local runtime profile using the fast cached probe path
cargo run -p ghost-link -- probe local-node fast

# Detect the local runtime profile using the deeper full probe path
cargo run -p ghost-link -- probe local-node full

# Run end-to-end multi-host 30B planning flow
cargo run -p ghost-link -- flow iprada-16gb zenbook-32gb 32 32
# Includes pipeline stage plan, token schedule preview, and measured execution throughput/latency metrics
# Inter-stage runtime transport in this path is wired over TCP loopback
# Optional execution tuning args: [exec_tokens] [micro_batch] [transport=tcp|inmem]
cargo run -p ghost-link -- flow iprada-16gb zenbook-32gb 32 32 128 4 tcp
cargo run -p ghost-link -- flow iprada-16gb zenbook-32gb 32 32 128 4 inmem

# TCP transport hardening knobs
GHOSTLINK_TCP_MAX_INFLIGHT=256 \
GHOSTLINK_TCP_RECONNECT_ATTEMPTS=3 \
GHOSTLINK_TCP_RECONNECT_BACKOFF_MS=10 \
GHOSTLINK_TCP_AUTH_TOKEN=example-token \
cargo run -p ghost-link -- flow iprada-16gb zenbook-32gb 32 32 128 4 tcp

# Optional: quick startup sweep to auto-select max inflight for this host
GHOSTLINK_TCP_AUTOTUNE=1 \
GHOSTLINK_TCP_AUTOTUNE_RUNS=3 \
GHOSTLINK_TCP_AUTOTUNE_CANDIDATES=32,64,128,256 \
cargo run -p ghost-link -- flow iprada-16gb zenbook-32gb 32 32 128 4 tcp

# Optional: persist autotune choice and reuse by plan/profile key
GHOSTLINK_TCP_AUTOTUNE=1 \
GHOSTLINK_TCP_AUTOTUNE_CACHE=./tmp/tcp_autotune_cache.tsv \
cargo run -p ghost-link -- flow iprada-16gb zenbook-32gb 32 32 128 4 tcp

# Force a cache refresh during rollout validation
GHOSTLINK_TCP_AUTOTUNE=1 GHOSTLINK_TCP_AUTOTUNE_REFRESH=1 \
cargo run -p ghost-link -- flow iprada-16gb zenbook-32gb 32 32 128 4 tcp

# Emit machine-readable runtime telemetry for dashboards/CI parsing
# Includes per-stage compute/wait plus bridge write/read timing metrics
GHOSTLINK_FLOW_METRICS_JSON=./tmp/flow-metrics.json \
cargo run -p ghost-link -- flow iprada-16gb zenbook-32gb 32 32 128 4 tcp

# Validate runtime metrics against SLO thresholds
python3 scripts/validate_flow_metrics.py \
	--file ./tmp/flow-metrics.json \
	--transport tcp \
	--profile production

# Generate repeatable perf snapshots for tcp/inmem and export summary JSON
# Warmup runs reduce cold-start noise and are excluded from summary statistics.
python3 scripts/flow_perf_snapshot.py --runs 5 --warmup-runs 1 --output-dir ./tmp/perf_snapshot

# Compare snapshot against committed baseline and fail on relative regressions
python3 scripts/check_perf_drift.py \
	--baseline ./docs/PERF_BASELINE.json \
	--current ./tmp/perf_snapshot/summary.json

# Stress profile baseline (512 tokens, micro-batch 8)
python3 scripts/check_perf_drift.py \
	--baseline ./docs/PERF_BASELINE_STRESS.json \
	--current ./tmp/perf_snapshot_stress/summary.json

# Analyze per-stage bridge timing distributions across runs
python3 scripts/analyze_flow_stage_metrics.py --glob './tmp/perf_snapshot_stress/tcp-*.json'

# Enforce stage-tail SLOs (p95) for transport bridge/wait timing
python3 scripts/validate_stage_tail_metrics.py --glob './tmp/perf_snapshot_stress/tcp-*.json'

# Enforce canary guardrails from summary (throughput, tail latency, spread)
python3 scripts/validate_flow_canary.py --summary ./tmp/perf_snapshot_stress/summary.json --profile stress

# Optional: temporarily override thresholds instead of baseline policy defaults
python3 scripts/check_perf_drift.py \
	--baseline ./docs/PERF_BASELINE.json \
	--current ./tmp/perf_snapshot/summary.json \
	--max-throughput-drop-ratio 0.30 \
	--max-p95-rise-ratio 0.60

# Verify Hugging Face model availability and download access
python3 -m pip install huggingface_hub
python3 scripts/verify_hf_models.py

# Verify specific model repos and file presence/downloadability
python3 scripts/verify_hf_models.py --repo mistralai/Mistral-7B-v0.1 --file config.json

# Export Criterion means for trend dashboards/artifacts
python3 scripts/summarize_criterion_report.py --criterion-root target/criterion --output artifacts/criterion-summary.json
```

The Mohawk GUI sources are vendored under [third_party/mohawk_gui](third_party/mohawk_gui). Use the `ghost-link gui` command to launch it from this repository.
Use `ghost-link gui-check` for readiness checks and `ghost-link gui-diagnose --strict` for categorized failure diagnostics suitable for CI artifacts.

Fast mode uses cheap local signals and a short-lived cache. Full mode enables deeper hardware probing, including external tools when they are available on the host.

## Probe Modes

- `fast`: cheap local detection intended for frequent runtime use
- `full`: deeper hardware inspection intended for operator diagnostics

If the host does not provide tools such as `nvidia-smi` or `lspci`, full mode falls back to the same sysfs and local signals available to fast mode.

## Docs Index

- [README.md](README.md): quick start, commands, and project status.
- [TESTING.md](TESTING.md): test structure, validated commands, and known gaps.
- [VERIFICATION.md](VERIFICATION.md): current verification scope and totals.
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md): component-level architecture and responsibilities.
- [docs/EXAMPLES.md](docs/EXAMPLES.md): runnable CLI and API usage examples.
- [docs/TROUBLESHOOTING.md](docs/TROUBLESHOOTING.md): operational troubleshooting and debugging tips.
- [CONTRIBUTING.md](CONTRIBUTING.md): contributor setup and pre-PR checks.
- [CHANGELOG.md](CHANGELOG.md): release-oriented change history.

## Repository Layout

```text
Ghostlink/
├── crates/
│   ├── ghostlink-core/
│   │   ├── src/
│   │   │   ├── accelerator.rs
│   │   │   ├── cluster.rs
│   │   │   ├── dashboard.rs
│   │   │   ├── health.rs
│   │   │   ├── host.rs
│   │   │   ├── lib.rs
│   │   │   ├── load_balance.rs
│   │   │   ├── planning.rs
│   │   │   ├── protocol.rs
│   │   │   ├── ring.rs
│   │   │   └── xdp.rs
│   │   └── tests/
│   │       ├── common.rs
│   │       └── integration.rs
│   └── ghost-link/
│       └── src/main.rs
├── benches/
├── docs/
├── TESTING.md
└── README.md
```

## Current Validation

The current workspace validation passes:

- `cargo test --workspace`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `python3 scripts/verify_hf_models.py`

That currently covers 112 passing tests across CLI, library, and package-owned integration targets.

## Benchmark Notes

Recent measured results in this environment include:

- `autotune/detect_runtime_profile_fast`: about `187-217 ns`
- `autotune/detect_runtime_profile_full`: about `192-203 us`
- `planning/80_layers_8_nodes_autotuned`: about `877-954 ns`
- `autotune/load_balance_80_layers_autotuned`: about `2.42-2.68 us`

The fast probe path is intended for frequent runtime use. The full probe path is intentionally slower.

## Flow Runtime Performance Snapshot

Measured on 2026-06-27 in this development container (5 runs per mode, `exec_tokens=256`, `micro_batch=4`) using:

```bash
for mode in tcp inmem; do
  for i in 1 2 3 4 5; do
		GHOSTLINK_FLOW_METRICS_JSON=./tmp/flow-${mode}-${i}.json \
			cargo run -p ghost-link -- flow iprada-16gb zenbook-32gb 32 32 256 4 $mode
  done
done

# Equivalent helper command
python3 scripts/flow_perf_snapshot.py --runs 5 --warmup-runs 1 --output-dir ./tmp/perf_snapshot
```

### Baseline (Before Transport Batching)

| Mode | Avg Throughput (tokens/s) | Min Throughput | Max Throughput | Avg P95 Latency (ms) | Avg Wall Time (ms) |
| --- | ---: | ---: | ---: | ---: | ---: |
| tcp | 14,320.94 | 11,066.67 | 16,771.44 | 17.74 | 18.26 |
| inmem | 142,486.76 | 95,300.38 | 213,641.33 | 1.73 | 1.92 |

### After Write Batching + Buffered Read/Write

| Mode | Avg Throughput (tokens/s) | Min Throughput | Max Throughput | Avg P95 Latency (ms) | Avg Wall Time (ms) |
| --- | ---: | ---: | ---: | ---: | ---: |
| tcp | 64,863.34 | 52,884.30 | 80,983.95 | 3.86 | 4.06 |
| inmem | 100,604.80 | 75,692.73 | 126,972.55 | 2.47 | 2.64 |

### Delta (After vs Baseline)

| Mode | Throughput | P95 Latency | Wall Time |
| --- | ---: | ---: | ---: |
| tcp | +352.9% | -78.2% | -77.8% |
| inmem | -29.4% | +42.8% | +37.5% |

### Improvement Priorities

1. Recover in-memory regression while keeping TCP gains.
	The transport optimization significantly improved TCP mode but inmem dropped. Isolate shared serialization/path overhead and keep transport-specific logic out of inmem execution.
2. Narrow remaining TCP gap to inmem.
	TCP is now much closer but still lower throughput and higher p95. Next targets: frame reuse with pooled buffers, larger batch payloads, and reducing per-batch allocation churn.
3. Improve benchmark determinism before setting tighter SLOs.
	Placement and host-profile variability can skew mode comparisons. Pin a deterministic benchmark profile and fixed assignment layout for apples-to-apples trend tracking.
4. Tighten CI regression gates with rolling baselines.
	Keep absolute floors and maintain mode-specific relative drift policies in docs/PERF_BASELINE.json.

## Limitations

- AF_XDP/eBPF remains Linux-specific and is not backed by kernel integration tests yet
- Full hardware probing depends on the tools and kernel interfaces available on the host
- Health monitoring currently evaluates node health from collected cluster metrics and heartbeat state, not direct network ping probes

## Documentation

- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
- [docs/EXAMPLES.md](docs/EXAMPLES.md)
- [docs/TROUBLESHOOTING.md](docs/TROUBLESHOOTING.md)
- [TESTING.md](TESTING.md)
- [CONTRIBUTING.md](CONTRIBUTING.md)

## License

Ghost-Link is released under the MIT License.
