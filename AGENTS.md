# Ghostlink State — July 15, 2026 (Session 6)

## Last Commit
`HEAD` — "feat: comprehensive reliability improvements for GUI API calls and interactions"

## What Was Done (Session 6 — July 15)

### Phase 1: API Client Hardening (`ghostlink_gui_modern/src/api.ts`)
- **Retry logic**: Exponential backoff (3 retries, 1s base, 30s max) for 5xx, 429, 408 errors
- **Circuit breaker**: Opens after 5 failures, 30s timeout, half-open after 2 successes
- **Request deduplication**: Identical GET requests within 5s window share single response
- **URL validation**: Trims whitespace, validates protocol/host — fixes trailing space bug
- **Structured errors**: Typed `ApiError` with status, code, retryable flag

### Phase 2: Launch Script Hardening
- **`launch-complete.bat`**: Pre-flight validation (URL format, required commands), trims `VITE_GHOSTLINK_API_BASE`, waits for `/api/health` endpoint
- **`launch-complete.sh`**: Same validation + mirror download support (hf-mirror.com), resume capability (Range headers), SHA256 verification
- **`launch.sh`**: Added `/api/health` readiness check

### Phase 3: Frontend Error Boundaries & Retry UI
- **`ErrorBoundary`**: Catches React errors, shows retry button + error details
- **`OfflineBanner`**: Auto-shows on network disconnect, auto-hides on reconnect
- **`useApiRetry` hook**: Generic retry wrapper with configurable backoff
- **`useOnlineStatus`**: Browser online/offline event listener
- **`useApi`**: Retry-wrapped versions of all API methods

### Phase 4: Backend Resilience (`crates/ghost-link/src/main.rs`)
- **`/api/health` endpoint**: Returns `gpu_available`, `inference_backend`, `native_engine`
- **`/health` endpoint**: Enhanced with GPU availability detection (NVIDIA/AMD/Apple)
- **Model downloads**: Mirror fallback (hf-mirror.com), HTTP Range resume, checksum verification
- **Metrics**: Added `gpu_available` field for graceful degradation

### Phase 5: Integration Tests & Monitoring
- **`tests/integration/reliability.test.ts`**: 16 tests for URL validation, retry delays, retryable errors
- **`src/config.test.ts`**: 21 tests for Zod config schema validation
- **All existing tests pass**: 94 frontend + 28 backend = 122 total

### Phase 6: Config Validation & Health Checks
- **`src/config.ts`**: Zod schema for all 25 settings with validation rules
- **`validateEnvVars()`**: Runtime check for `VITE_GHOSTLINK_API_BASE` format

## Build Verification
- `npx vitest run` — **94/94 passed**
- `npx tsc --noEmit` — **OK**
- `cargo fmt --all --check` — **OK**
- `cargo clippy --workspace --all-targets -- -D warnings` — **OK**
- `cargo test --workspace` — **122/122 passed**

## Known Issues
1. `Qwen3.5-4B-BF16.gguf` is corrupt — renamed to `.bak`
2. ~~`/api/metrics` hangs~~ — fixed: background host sampler + real tok/s/p50/p95 from chat
3. Worker networking is local-only — discovery only detects same LAN
4. No auth enforcement — discovery token configured but not enforced
5. Linux OOM with large models — default models now tiny (15M/1.1B); avoid loading 7B+ without sufficient RAM
6. Download from HuggingFace may fail — if `GHOSTLINK_INSECURE_TLS=1` helps, TLS cert bundle may need updating

## To Restart
```powershell
cd C:\Users\rwill\Ghostlink
.\launch.bat
```

For Linux:
```bash
cd ~/Ghostlink
./launch.sh
```

**Ports:** GUI → API `:8003` only. Inference is `:8080` (llama-server) or `:11434` (ollama).
Pointing the GUI at `:8000`/`:8080` caused **405** on chat/models — fixed in unified launchers.

## Models on Disk
- `models/stories15M-q4_0.gguf` (~19 MB)
- `models/tinyllama-1.1b-chat-v1.0.Q2_K.gguf` (~483 MB)