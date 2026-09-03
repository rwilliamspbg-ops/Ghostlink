# Deployment Guide

This guide provides a progressive path for deploying Ghost-Link:

1. Single-node validation
2. Small local cluster (single host, multi-process)
3. Multi-host LAN cluster (staged production rollout)

## Prerequisites

- Rust toolchain installed (`stable`)
- Linux hosts for best coverage of runtime and networking features
- Optional GUI dependencies if you need desktop control paths

Build the CLI:

```bash
cargo build --workspace
```

## Stage 1: Single-Node Validation

Run local quality and readiness checks first:

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p ghost-link -- gui-check --strict
cargo run -p ghost-link -- probe local-node fast
```

Run a local flow smoke test:

```bash
GHOSTLINK_FLOW_METRICS_JSON=./tmp/flow-metrics-single.json \
cargo run -p ghost-link -- flow local-node local-node 32 32 128 4 tcp
```

Validate metrics:

```bash
python3 scripts/validate_flow_metrics.py --file ./tmp/flow-metrics-single.json --transport tcp --profile production
```

## Stage 2: Local Multi-Node Cluster (Single Host)

Use the built-in orchestration helper:

```bash
cargo run -p ghost-link -- cluster-start 3 46000
```

This launches multiple local listeners on loopback ports and validates join/reply behavior.

### Optional: Config-Driven Launch

```bash
cargo run -p ghost-link -- --config ./ghostlink.example.toml cluster-start
```

Or with environment fallback:

```bash
GHOSTLINK_CONFIG=./ghostlink.example.toml cargo run -p ghost-link -- cluster-start
```

## Stage 3: Multi-Host LAN Rollout

Run listeners on each node:

```bash
cargo run -p ghost-link -- listen node-a
```

On control node, probe/join:

```bash
cargo run -p ghost-link -- join controller-node
```

Recommended rollout sequence:

1. Start with 2 nodes and validate discovery stability.
2. Enable runtime flow smoke for tcp/inmem.
3. Run perf snapshot + drift checks.
4. Expand to 4+ nodes only after stable daily runs.

## Stage 3b: Real Cross-Machine Flow Execution

`flow` on its own only ever runs in one process — even when you give it a
`remote_id`, without more it registers that node as a label and simulates
its stage locally. To make `flow` actually reach a second machine, pass
`--remote-addr` and have a `stage-worker` process already listening there.

**On the remote machine** (or a second terminal/container on the same
host for a quick local check), start the worker and give it the address to
bind:

```bash
cargo run -p ghost-link -- stage-worker 0.0.0.0:9500
```

The worker accepts exactly one coordinator connection, runs whatever stage
it's assigned, prints a summary, and exits — the same one-shot lifecycle
`cluster-start`'s child listeners already use. Start a fresh one for each
run.

**On the coordinator**, point `flow` at that address:

```bash
cargo run -p ghost-link -- flow local-node remote-node 32 32 128 4 tcp \
  --remote-addr <remote-host>:9500
```

Watch for `REAL execution: ...` in the coordinator's output — if you
instead see `SIMULATED execution: ...`, `--remote-addr` was omitted, not
reachable, or mistyped, and nothing left this machine.

**What this does and doesn't prove**: the transport is genuinely
cross-process — a real outbound `TcpStream::connect`, real framed
batches, real measured round-trip latency fed back into the cluster's
health metrics (not the placeholder `record_latency(3.2)` constants the
simulated path seeds). What it does *not* prove is distributed LLM
inference: the per-stage compute (`run_stage_compute`) is a synthetic
timing proxy — sine-wave arithmetic scaled to look like layer/device
cost, not real model math. A real cross-machine `flow` run demonstrates
the pipeline-splitting and networking layer working end-to-end; it is not
evidence of correct distributed generation output.

An automated version of this same two-process check lives in
[../crates/ghost-link/tests/stage_worker_integration.rs](../crates/ghost-link/tests/stage_worker_integration.rs),
which spawns both binaries as real child processes and asserts on the
`REAL execution` path.

For a containerized two-machine-shaped example, see
[../docker-compose.test-fabric.yml](../docker-compose.test-fabric.yml) —
`ghostlink-worker-2` runs `stage-worker` and `ghostlink-coordinator` runs
`flow --remote-addr` against it across the compose network, closer to a
real multi-host topology than loopback.


## Production Status & Hardening Caution

> [!CAUTION]
> **Pre-Production Status**: Treat current release status as **pre-production** until multi-host LAN soak tests are green. While capacity splitting across heterogeneous nodes is verified, network-bound tokens-per-second throughput optimization and zero-touch cluster orchestration remain in progress.

## Production LAN Deployment Checklist

Before deploying Ghostlink across a multi-host LAN for production or team workloads, verify every item in this checklist:

### 1. Build & Version Matching
- [ ] **Identical Ghostlink and llama.cpp Builds**: Ensure every node runs the exact same Ghostlink binary and `llama.cpp` commit (`ggml-rpc` protocol).
  > **Why**: Upstream `ggml-rpc` does not perform internal version negotiation. Version-mismatched peers can silently corrupt tensor computations on larger models. Ghostlink peer discovery checks `rpc_build_id` and excludes confirmed mismatches, but all nodes should be compiled from the identical commit.

### 2. RPC Peer Authentication & Network Access
- [ ] **Configure Shared Secret (`rpc_shared_secret`)**: Set `rpc_shared_secret` in `ghostlink.toml` (or `GHOSTLINK_RPC_SHARED_SECRET` environment variable) identically across all cluster nodes.
  > **Why**: When configured, Ghostlink requires a random-nonce HMAC-SHA256 handshake over a challenge port before granting temporary IP admission to a contributor.
- [ ] **Restrict Allowed Peers (`rpc_allowed_peers`)**: Define `rpc_allowed_peers` in `ghostlink.toml` with explicit IPv4 addresses or CIDR subnets (e.g. `["192.168.1.0/24"]`).
  > **Why**: An empty list exposes the RPC port to any host on the network. The allowlist proxy ensures only trusted IP ranges can connect.

### 3. Opt-In Compute Contribution
- [ ] **Default-Off Contributor Mode**: Confirm `contribute_compute = false` by default on all nodes.
  > **Why**: Compute contribution must be an explicit operator choice. Nodes intended as contributors must set `contribute_compute = true` to spawn the local `ggml-rpc-server` worker.

### 4. Contributor Lifecycle & Loss Behavior
- [ ] **Supervised Worker Process**: Understand that `ggml-rpc-server` is supervised by Ghostlink (`RpcSupervisor`).
  > **What happens when a contributor process crashes**: Ghostlink immediately revokes the node's discovery advertisement (`contribute_compute` set to false, `excluded_reason: "rpc child not running"`) and stops routing new inference requests to it until auto-restart succeeds.
- [ ] **Drain and Restart on Contributor Loss**:
  > **What happens if a contributor dies mid-request**: If an active RPC peer disappears during model load or generation, Ghostlink drains non-viable server topology, cancels the request cleanly, and logs a contributor loss error rather than producing corrupted output. *Live mid-generation token state migration across nodes remains an open roadmap item.*

### 5. Network Architecture & Port Exposure
Verify firewall rules and bindings match Ghostlink's system port architecture:

| Port | Service | Exposure | Description |
|---|---|---|---|
| `:8000` | **Go Control Plane** | **Public / Browser** | **Single public entry point** for Web GUI and external clients. Handles CORS and proxies API calls internally. |
| `:8003` | **Rust Internal API** | Internal Loopback / Private | Internal engine API. Requires Bearer API key authentication. |
| `:8080` | `llama-server` | Internal Loopback | Local native inference engine instance. |
| `:50052` | `ggml-rpc-server` | LAN (allowlisted) | Distributed RPC tensor execution worker port. |
| `:11134` | UDP/mDNS Discovery | LAN | Cluster auto-discovery and HMAC challenge auth port. |

> [!IMPORTANT]
> **GUI Port Restriction**: The Web GUI **NEVER** connects directly to port `:8003`. All browser requests must target the Go control plane on port `:8000`. Direct browser calls to `:8003` will fail due to CORS or route restrictions.

## Configuration File Usage

Ghost-Link supports TOML config defaults via:

- `--config <path>`
- `GHOSTLINK_CONFIG=<path>`
- `./ghostlink.toml` auto-load (if present)

Use [../ghostlink.example.toml](../ghostlink.example.toml) as a baseline.

## systemd Deployment (Linux)

Template unit file:

- [../deploy/systemd/ghost-link-listener@.service](../deploy/systemd/ghost-link-listener@.service)

Install on host:

```bash
sudo cp deploy/systemd/ghost-link-listener@.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now ghost-link-listener@node-a.service
```

Inspect logs:

```bash
journalctl -u ghost-link-listener@node-a.service -f
```

## Containerized Local Deployment

Reference compose file:

- [../deploy/docker/docker-compose.local.yml](../deploy/docker/docker-compose.local.yml)

Run:

```bash
docker compose -f deploy/docker/docker-compose.local.yml up --build
```

## Operational Validation Checklist

- `CI` and `Production Gate` checks are green.
- `scripts/fault_injection_matrix.py --strict` passes.
- `scripts/active_network_probe.py` shows acceptable failure ratio.
- `scripts/check_perf_drift.py` passes against active baseline.

## Production Cautions

- Treat current release status as pre-production unless multi-host LAN soak tests are green.
- For sensitive networks, pair deployment with segmented networks and stronger transport controls.
- AF_XDP paths require host/kernel/NIC compatibility validation beyond container preflight.
