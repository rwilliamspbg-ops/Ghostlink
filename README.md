# Ghostlink

Ghost-Link is an open-source scaffold for a zero-config LAN fabric that turns spare local GPUs into a shared execution surface for large-model inference and training.

This repository now provides a minimal Rust monorepo foundation for the blueprint in the issue:

- a bounded single-producer/single-consumer ring buffer suitable for zero-copy hand-off experiments,
- raw discovery frame encoding with a dedicated Ghost-Link EtherType,
- a sequential VRAM-aware layer placement engine with quantization fallback thresholds,
- and a terminal-friendly ASCII dashboard/CLI scaffold for demos and future operator workflows.

## Workspace layout

```text
crates/
  ghostlink-core/  # shared runtime primitives
  ghost-link/      # CLI demo entrypoint
```

## What is implemented

### Phase 1 foundations

- **Kernel-bypass integration scaffold:** `ghostlink-core` defines the transport-facing contracts needed to plug in AF_XDP/eBPF work without first committing to a kernel or driver binding.
- **SPSC ring buffer:** `crates/ghostlink-core/src/ring.rs` provides a bounded lock-free queue for single producer / single consumer DMA-style hand-off.
- **Node discovery protocol:** `crates/ghostlink-core/src/protocol.rs` defines a custom EtherType (`0x88B5`) and join/discovery/attestation frame kinds with resource advertisement payloads.
- **Cluster state:** `crates/ghostlink-core/src/cluster.rs` tracks discovered node capabilities (VRAM, system memory, compute capability).

### Phase 2 foundations

- **Greedy layer splitting:** `crates/ghostlink-core/src/planning.rs` assigns layers sequentially across nodes by VRAM capacity, matching the shape of the Llama/Mixtral examples in the product blueprint.
- **Adaptive quantization trigger:** the same planner exposes `select_quantization_mode` to fall back from raw tensors to 8-bit or 4-bit transport when delivery health drops.

### Phase 3 foundations

- **Terminal dashboard scaffold:** `crates/ghostlink-core/src/dashboard.rs` renders a live ASCII cluster summary suitable for a future ratatui/cursive front-end.
- **Demo CLI:** `cargo run -p ghost-link -- <command>` exposes `plan`, `join`, and `dashboard` commands to exercise the current architecture.

## Quick start

```bash
cargo test
cargo run -p ghost-link -- plan
cargo run -p ghost-link -- join node-02
cargo run -p ghost-link -- dashboard
```

## Why Ghost-Link

Ghost-Link is designed around a simple idea: consumer machines on the same wire should be able to pool VRAM and move tensors directly, instead of forcing every meaningful AI workload through a cloud subscription or heavyweight orchestration layer.

The long-term goal is a copy-paste local clustering workflow where a workstation, gaming rig, and rack server can discover each other, advertise resources, and participate in a shared execution graph with a single command.

## License

Ghost-Link is released under the MIT License to keep experimentation and enterprise adoption friction low.