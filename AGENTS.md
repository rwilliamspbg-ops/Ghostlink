# Ghostlink State — July 14, 2026 (Session 3)

## Last Commit
`fa991e6` — "fix: prevent OOM on integrated GPUs, use CPU-safe default NGL"

## What Was Fixed (Session 3 — July 14)

### OOM on Integrated GPUs (`crates/ghost-link/src/main.rs`)
- **Default `ngl`** changed from `-1` (all layers GPU) → `0` (CPU-only) — prevents out-of-memory on systems with 16GB shared RAM / integrated GPUs.
- **`GHOSTLINK_LLAMA_NGL` env var** now read in `handle_gui_model_load()` so launch scripts can override the default at runtime.
- Updated `README.md` default for `GHOSTLINK_LLAMA_NGL`.

### Linux Launch Scripts (`launch-complete.sh`, `launch.sh`)
- Both scripts now export `GHOSTLINK_LLAMA_NGL` to the Rust backend, making hardware-detected GPU layer count respected when loading models via the UI.
- `launch-fast.sh` and `launch-splash.sh` delegate to `launch-complete.sh` automatically.

### Download Flow (Session 2 continued)
- **`<tr>`/`<td>` → `<div>` layout** in `ModelsTab.tsx` — the `<tr>` was oustide `<table>`, making the download button unclickable.
- **HF API field** `modelId` → `id` — the upstream HuggingFace API returns the model ID under `"id"`, not `"modelId"`.
- **GGUF magic-byte validation** — rejects non-GGUF files before download.
- **Partial download detection** — existing `.gguf` files smaller than Content-Length are redownloaded instead of reused.

### Earlier Fixes (Sessions 1 & 2)
- P2P networking endpoints (`/api/workers/add`, `discover`, `disconnect`)
- AMD GPU metrics via PowerShell Performance Counters
- Runtime selector clickable cards in SettingsTab
- Production build in `launch-complete.bat`
- `cargo fmt`, clippy, tests — all green after each session

## Build Verification
- `cargo fmt --all --check` — **OK**
- `cargo clippy --workspace --all-targets -- -D warnings` — **OK**
- `cargo test --workspace` — **169/169 passed** (28 ghost-link + 87 ghostlink-core unit + 7 common + 28 integration + 19 multinode)
- `python3 scripts/validate_gui_api_contract.py` — **passed** (13 endpoints, 18 routes)

## Known Issues (Not Fixed Yet)
1. **Worker networking is local-only** — Background discovery only detects nodes on the same LAN segment. WAN/mesh networking requires the TCP transport layer.
2. **No auth enforcement** — Discovery auth token is configured but not enforced by default (`enforce_auth: false`).
3. **`production-gate-verdict.json`** missing locally — `validate_production_gate_verdict.py` skipped; needs CI-generated artifact.
4. **`cargo audit` / `pip-audit`** — Not run (tools not installed on this machine).
5. **License consistency script** — Fails on Windows bash (`set -o pipefail` unsupported).

## To Restart
```powershell
cd C:\Users\rwill\Ghostlink

# Start backend
.\target\release\ghost-link.exe serve 127.0.0.1 8003

# Start frontend (separate terminal)
cd ghostlink_gui_modern
npm run dev -- --host 127.0.0.1 --port 5173

# Or use the all-in-one script:
$env:GHOSTLINK_SKIP_BUILD=1; $env:GHOSTLINK_SKIP_MODEL=1; .\launch-complete.bat
```

For Linux:
```bash
cd ~/Ghostlink
GHOSTLINK_SKIP_BUILD=1 GHOSTLINK_SKIP_MODEL=1 bash launch-complete.sh
```

## Models on Disk
- `models/stories15M-q4_0.gguf` (~19 MB, loaded and tested)
- `models/tinyllama-1.1b-chat-v1.0.Q2_K.gguf` (~483 MB)
