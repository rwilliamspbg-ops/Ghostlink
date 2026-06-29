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
