# Ghostlink State — July 14, 2026

## Last Commit
`4660270` — "fix: hardware detection, GPU metrics, frontend bugs, and download progress"

## What Was Fixed (Session 2 — July 14)

### P2P Networking (`crates/ghost-link/src/main.rs`)
- **`/api/workers/add`** — Now performs TCP health check (`GET /health`) before adding; rejects duplicates; returns `reachable` status.
- **`/api/workers/discover`** — Now reads from `ClusterState.nodes()` (populated by background UDP discovery thread) instead of hardcoded `{ count: 2 }`.
- **`/api/workers/:worker_id/disconnect`** — Now actually removes the worker from `backend.workers` vec instead of no-op.

### AMD GPU Metrics (`crates/ghost-link/src/main.rs`)
- **`collect_system_metrics()`** — Replaced `CurrentRefreshRate` fallback with two PowerShell Performance Counter queries:
  1. `Get-Counter "\GPU Engine(*eng_ver*)\Utilization Percentage"` (primary)
  2. `Get-CimInstance Win32_PerfFormattedData_GPUPerformanceCounters_GPUEngine` (fallback)

### Runtime Selector (`ghostlink_gui_modern/src/components/SettingsTab.tsx`)
- Hardware detection cards now **clickable** — tapping an available runtime calls `/api/runtime/select` and shows "Active" state.
- Added `selectRuntime()` method to `api.ts`.
- Real-time status feedback on runtime switch.

### Production Build (`launch-complete.bat`)
- GUI now builds with `npm run build` and serves from `dist/` via `vite preview`.
- Falls back to dev server only if build fails.

### Build Verification
- Rust: `cargo build -p ghost-link --release` — **OK**
- Frontend: `npm run build` — **OK** (267 KB gzip)
- Core tests: `cargo test -p ghostlink-core --release` — **141/141 passed**

## Hardware Detected
- CPU Cores: 16 (Ryzen AI 7 350 has 8 cores / 16 threads)
- System RAM: 28 GB
- GPU: AMD Radeon™ 860M Graphics (DirectML backend)
- NPU: AMD Ryzen AI 7 350 w/ Radeon 860M

## Current Services
- llama-server: `http://127.0.0.1:8080`
- Backend API: `http://127.0.0.1:8003`
- Frontend GUI: `http://127.0.0.1:5173`

## Known Issues (Not Fixed Yet)
1. **Model lifecycle test** — Download → load → unload pipeline is wired end-to-end but has not been integration-tested on this hardware.
2. **Worker networking is local-only** — Background discovery only detects nodes on the same LAN segment. WAN/mesh networking requires the TCP transport layer.
3. **No auth enforcement** — Discovery auth token is configured but not enforced by default (`enforce_auth: false`).

## To Restart Tomorrow
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

## Models on Disk
- `models/stories15M-q4_0.gguf` (~19 MB, loaded and tested)
- `models/tinyllama-1.1b-chat-v1.0.Q2_K.gguf` (~483 MB)
