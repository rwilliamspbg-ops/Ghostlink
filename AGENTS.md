# Agent Instructions

Repo-wide guidance for AI coding agents (and human contributors) working in
Ghostlink. This file is about how to work in this codebase — for what the
project is and how to run it, see [README.md](README.md); for the full
contributor workflow, see [CONTRIBUTING.md](CONTRIBUTING.md).

## Layout

- `crates/ghost-link` — CLI + OpenAI-compatible API server (Rust).
- `crates/ghostlink-core` — shared runtime primitives: planning, routing,
  transport, health monitoring, system profiling. `ghost-link` depends on it.
- `crates/mcp-calculator`, `crates/mcp-rag`, `crates/mcp-vision`,
  `crates/mcp-memory` — local stdio MCP servers used by the chat tool-calling
  loop. Internal (`publish = false`), not published to crates.io.
  `mcp-memory` is the assistant's durable SQLite-backed memory store; its tools
  are workspace-scoped, and `capability::stamp_workspace_scope` overwrites the
  scope argument at dispatch so a model cannot address another workspace.
- `crates/ghostlink-gui` — an earlier Tauri/Svelte desktop shell, the
  pre-React GUI prototype. **Legacy only** — not the active GUI, and not
  wired into the launchers, workspace, or release pipeline. If you're
  looking for how something is done in the current UI, it's almost
  certainly already implemented in `ghostlink_gui_modern/`; check there
  first, and don't build new features here without checking with a
  maintainer.
- `ghostlink_gui_modern/` — the active GUI (React + Vite + TypeScript). Every
  launcher (`launch.sh`, `launch.bat`) builds and serves this one.
- `control-plane/` — Go gateway in front of the Rust API: CORS, rate
  limiting, request logging, streaming-safe proxying. The GUI talks to this,
  not directly to `ghost-link`'s API port.
- `sdks/python`, `sdks/js` — client SDKs mirroring each other's shape.
- `docs/` — user-facing reference docs (architecture, API, deployment,
  benchmarks, etc.) plus `docs/archive/` for superseded status documents.

## Before opening a PR or considering work done

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

If you changed the GUI (`ghostlink_gui_modern/`):

```bash
npx tsc --noEmit
npx eslint .
npx vitest run
```

CI enforces: cargo fmt, clippy, miri, loom, fuzzing, cargo-deny, govulncheck (Go),
SDK tests (JS + Python), ESLint, and benchmark regression (on PRs touching
transport/ring-buffer code).

If you changed transport, ring buffer, or pipeline code, also run
`cargo bench --package ghostlink-core` and report before/after numbers — this
is performance-sensitive code with documented microbenchmark baselines (see
[docs/BENCHMARKS.md](docs/BENCHMARKS.md), which also records how noisy the
dev hardware is and why multi-run distributions beat single-run deltas).
For end-to-end pipeline throughput rather than primitives, use
`scripts/flow_perf_snapshot.py`; for real cross-process/cross-machine
numbers, `scripts/remote_flow_benchmark.py` (both documented in
BENCHMARKS.md's "Benchmark Methodology" section).

If you changed hardware detection or system profiling, run
`cargo run -p ghost-link -- probe my-node --full` and check for regressions.

Full pre-push checklist, release rubric, and PR expectations live in
[CONTRIBUTING.md](CONTRIBUTING.md) — read it before larger changes.

## Conventions

- **Don't fabricate numbers.** This repo had a real incident with fabricated
  placeholder benchmark data checked in as if it were measured. Every number
  in `docs/BENCHMARKS.md` and the CHANGELOG is expected to be a real,
  reproducible measurement with hardware/method noted — never invent one.
- **Update the CHANGELOG for behavior changes.** Follow the existing
  entries' style: bold one-line summary, then specific file/function names
  and the *why*, not just the *what*.
- **Archive, don't delete, superseded status docs.** If a doc genuinely
  reflects a snapshot in time (a session log, a phase-completion report)
  rather than the current state of the project, move it to
  `docs/archive/legacy-root-docs/` and add it to
  [docs/archive/INDEX.md](docs/archive/INDEX.md), per
  [CONTRIBUTING.md](CONTRIBUTING.md#documentation-expectations). Don't leave
  it in the repo root or in `docs/` proper where it reads as current.
- **Ports**: the GUI talks to the control-plane gateway on `:8000` (the Go
  reverse proxy in `control-plane/` — CORS, rate limiting, request
  logging). The Rust API itself listens on `:8003` (internal — the GUI
  does not call it directly; it requires bearer auth and, with
  `enable_tls`, serves a self-signed loopback cert). Inference backends
  are `:8080` (native llama-server) or `:11434` (Ollama), reached through
  the gateway rather than by the browser directly. Pointing the GUI at
  the wrong port is the most common local-dev failure mode (usually a
  405, or a 401 if you land on `:8003` without credentials).

## Security-sensitive areas

Auth (`crates/ghost-link/src/auth.rs`), RBAC/scoped API keys, the audit
trail, and RPC peer authentication are security-critical paths — see
[docs/SECURITY_MODEL.md](docs/SECURITY_MODEL.md) for the current threat
model before changing anything here, and update that doc if the security
posture changes. Report vulnerabilities per [SECURITY.md](SECURITY.md)
rather than opening a public issue.

RPC peer admission is the same category: the IP allowlist and
`rpc_shared_secret` HMAC handshake live in
`crates/ghost-link/src/rpc_cluster.rs` (`validate_non_loopback_rpc_security`,
`ip_allowed`, `admit_via_secret`). Contributor-process supervision —
`RpcSupervisor`, surfaced through `is_contributing_healthy()` and
`get_rpc_supervisor_info()` — is what stops a node from advertising
`contribute_compute` over discovery while its `ggml-rpc-server` child is
dead (a real gap found on hardware; see [docs/BENCHMARKS.md](docs/BENCHMARKS.md)).
Changing either the admission path or the supervision loop is a security
change, not a routine refactor.
