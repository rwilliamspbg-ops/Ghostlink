# ghostlink-core

Core runtime primitives for [Ghostlink](https://github.com/rwilliamspbg-ops/Ghostlink):
discovery, cluster state, planning, load balancing, health monitoring, and transport
logic for distributed inference workloads across heterogeneous hardware. Depended on by
[`ghost-link`](../ghost-link) (the CLI/API binary); this crate has no binary of its own.

## What's in here

- **`discovery.rs` / `mdns.rs` / `protocol.rs`** — zero-config peer discovery over UDP
  broadcast and mDNS, and the wire format (`NodeResources`, `DiscoveryFrame`) nodes use to
  advertise hardware (VRAM, system memory, GPU name), RPC contribution capability
  (`rpc_port`), and — as of the version-mismatch-corruption fix this crate shipped —
  an `rpc_build_id` fingerprint so a coordinator can detect (and refuse to route
  distributed inference through) a peer running a different `llama.cpp` build than its
  own, rather than silently getting corrupted output. Both the shared binary encoder and
  the UDP-specific hand-duplicated one carry every field — a real historical bug here was
  a field silently dropped by only one of the two encoders, so each field gets its own
  round-trip regression test now, not just the shared helper.
- **`cluster.rs` / `health.rs`** — live cluster membership and per-node health tracking.
- **`planning.rs` / `load_balance.rs`** — placement planning and load distribution across
  discovered nodes.
- **`runtime.rs` / `ring.rs`** — the ring-buffer/TCP-bridge pipeline execution engine used
  by the synthetic `ghost-link flow`/`stage-worker` benchmark harness. Worth being
  explicit about what this is *not*: it moves synthetic `f32` timing payloads to prove out
  transport latency, not real model tensors. Real distributed inference goes through
  llama.cpp's own `ggml-rpc` backend instead (see `ghost-link`'s `rpc_cluster.rs`), which
  this crate's runtime doesn't implement — the two are genuinely separate mechanisms and
  conflating them was a real gap this project had to fix (see the main repo's
  `docs/ROADMAP.md`, "Priority Zero").
- **`system_profile.rs` / `accelerator.rs` / `host.rs`** — hardware detection (CPU, GPU,
  VRAM, NPU where available).
- **`circuit_breaker.rs`** — a 3-state (closed/open/half-open) circuit breaker with
  jittered exponential backoff, used on the TCP transport bridge's reconnect path so a
  chronically-unreachable node fails fast instead of repeating a full connect/backoff
  cycle on every call.
- **`api_response_cache.rs`** — TTL + ETag response caching (used for the model-list
  endpoint, which otherwise re-scans disk on every request).
- **`kv_cache.rs`** — KV-cache primitives (currently no caller in `runtime.rs`, since
  model execution is delegated to an external inference engine — kept as a tested
  primitive for future use).
- **`watcher.rs` / `xdp.rs` / `dashboard.rs` / `models.rs` / `autotune.rs`** — config file
  watching, optional AF_XDP kernel-bypass networking support, terminal dashboard data
  model, model registry types, and TCP transport autotuning.

## Feature flags

- `rocm` — AMD ROCm device support in the hardware-detection/accelerator paths.

## Testing

```bash
cargo test -p ghostlink-core
```

This crate's test suite is where most of the wire-protocol and cluster-logic regression
coverage lives, including the discovery round-trip tests referenced above.

## More

- [Main repository](https://github.com/rwilliamspbg-ops/Ghostlink) for the full stack,
  docs, and GUI.
- [`ghost-link`](https://crates.io/crates/ghost-link) is the CLI/API binary that actually
  uses this crate — most people want that, not this one, unless you're embedding
  Ghostlink's discovery/planning logic in your own tool.

## License

MIT — see [`LICENSE`](https://github.com/rwilliamspbg-ops/Ghostlink/blob/main/LICENSE) in
the main repository.
