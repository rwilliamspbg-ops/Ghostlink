# ghost-link

CLI and API runtime for [Ghostlink](https://github.com/rwilliamspbg-ops/Ghostlink) — a
distributed inference fabric that discovers heterogeneous consumer/prosumer hardware on a
LAN (gaming GPU, old laptop, NPU-equipped ultrabook) and turns it into one inference
cluster, with zero-config discovery and real cross-machine model sharding.

This crate is the thing you actually run: `ghost-link serve` starts an OpenAI-compatible
API server; other subcommands cover discovery, diagnostics, and the distributed-inference
CLI surface. It depends on [`ghostlink-core`](../ghostlink-core) for the underlying
protocol, planning, and transport primitives.

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/rwilliamspbg-ops/Ghostlink/main/scripts/install.sh | sh
```

Or from source: `cargo install --path .` from a checkout of the
[full repository](https://github.com/rwilliamspbg-ops/Ghostlink) (this crate isn't
useful installed standalone from crates.io without the rest of the repo's `models/`,
`ghostlink.toml`, and `third_party/llama.cpp` layout — see the root README's Quick Start).

## What it does

- **`ghost-link serve <host> <port>`** — starts the API server: an OpenAI-compatible
  surface (`/v1/chat/completions`, `/v1/completions`, `/v1/embeddings`, `/v1/models`)
  plus Ghostlink's native `/api/*` surface (workers, sessions, settings, metrics, real
  token-by-token streaming chat). Backed by either a local llama.cpp (`native`), a local
  Ollama instance (`ollama`), or vLLM, selected via `inference_backend` in settings.
- **Real distributed inference**, not a demo: when `distributed_inference: true` and
  another node on the LAN has opted in via `contribute_compute: true`, this node
  discovers it via UDP broadcast or mDNS (zero manual configuration), computes a
  VRAM-proportional `--tensor-split`, and launches its local `llama-server` with
  `--rpc host:port,...` — llama.cpp's own RPC backend (`ggml-rpc`) does the real
  cross-process tensor execution. See [`rpc_cluster.rs`](src/rpc_cluster.rs) for the
  implementation and its `SECURITY:` doc comment for what this does and doesn't protect
  against (an IP allowlist restricts *who* can submit compute jobs; it isn't
  protocol-level authentication — that's an upstream `ggml-rpc-server` limitation).
- **`ghost-link probe <node-id> [--full]`** — real hardware detection (CPU, GPU/VRAM,
  system RAM) for this machine or a named node.
- **`ghost-link doctor [--strict] [--json <path>]`** — environment/config sanity check.
- **`ghost-link flow` / `ghost-link stage-worker`** — the earlier synthetic pipeline
  benchmark harness (ring buffer / TCP transport timing, not real model inference —
  see `docs/BENCHMARKS.md` in the main repo for why this is a distinct thing from real
  distributed inference).
- **`ghost-link dashboard` / `ghost-link gui`** — the terminal dashboard and desktop GUI
  entry points.

## Settings

Runtime settings live in `settings.json` (path overridable via `GHOSTLINK_SETTINGS_PATH`)
and cover inference backend selection, model/context sizing, discovery, TLS, and the
distributed-inference fields (`distributed_inference`, `contribute_compute`, `rpc_port`,
`rpc_allowed_peers`). Most can also be read/changed live via `GET`/`POST /api/settings`.
See the root repo's `ghostlink.example.toml` and `docs/QUICKSTART.md` for a worked
example.

## Feature flags

- `cuda` — CUDA-specific paths.
- `rocm` — enables `ghostlink-core`'s `rocm` feature (AMD ROCm device support).

## Testing this crate specifically

```bash
cargo test -p ghost-link
```

For the real distributed-inference path specifically, see the root repo's
`docker-compose.rpc-fabric.yml` + `scripts/rpc_fabric_assert.py` (also the CI gate in
`.github/workflows/distributed-e2e.yml`) — a two-container fabric that proves real
cross-container inference (`real_inference: true`, live RPC connection log evidence),
not just that discovery found a peer.

## More

- [Main repository](https://github.com/rwilliamspbg-ops/Ghostlink) — full docs, GUI,
  Docker Compose stacks, and the rest of the workspace.
- [`docs/ARCHITECTURE.md`](https://github.com/rwilliamspbg-ops/Ghostlink/blob/main/docs/ARCHITECTURE.md),
  [`docs/DEPLOYMENT.md`](https://github.com/rwilliamspbg-ops/Ghostlink/blob/main/docs/DEPLOYMENT.md),
  [`docs/ROADMAP.md`](https://github.com/rwilliamspbg-ops/Ghostlink/blob/main/docs/ROADMAP.md)
  (competitive strategy + honest status of every distributed-inference claim, including
  real bugs found via live multi-machine testing).

## License

MIT — see [`LICENSE`](https://github.com/rwilliamspbg-ops/Ghostlink/blob/main/LICENSE) in
the main repository.
