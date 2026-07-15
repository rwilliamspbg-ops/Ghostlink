# Ghostlink State — July 15, 2026 (Session 5)

## Last Commit
`e161ac1` — "fix: resolve HF model download failures, Windows .bat trailing space, Linux OOM, and add form accessibility"

## What Was Done (Session 5 — July 15)

### Root Cause: Trailing Space in `launch-complete.bat`
Inline `set VITE_GHOSTLINK_API_BASE=... &&` had a space before `&&`, producing `http://127.0.0.1:8003 ` (trailing space). Axios constructed an invalid URL; every API call silently failed.

### Fixes Applied

**Launch Scripts:**
- `launch-complete.bat` — trailing space removed; switched `npm run build+preview` → `npm run dev`; `exit /b 1` on missing default model → warning
- `launch-complete.sh` — added `export VITE_GHOSTLINK_API_BASE`; model download failure now warns instead of `exit 1`
- `launch.sh` — added `export VITE_GHOSTLINK_API_BASE`

**Frontend:**
- `App.tsx` — default API base changed from `localhost:8003` → `127.0.0.1:8003`
- `vite.config.ts` — proxy target updated to `127.0.0.1:8003`
- `api.ts` — `searchHuggingFace` fallback returns `{ models: [] }`; `downloadModel` returns `{ success: false, error }`
- `ModelsTab.tsx` — `searchHF` only overwrites if non-empty; `refreshModels()` called after download POST; progress poll capped at 600 iterations
- `SessionsTab.tsx` — error message shown when API fails
- `SettingsTab.tsx`, `WorkersTab.tsx` — `id`/`name`/`htmlFor` on all form fields (accessibility)

**Backend:**
- `RuntimeDetector::detect()` moved to `tokio::task::spawn_blocking` with 30s timeout (fixes Settings tab hang)
- `download_hf_model`: added `.no_proxy()` to reqwest; added `GHOSTLINK_INSECURE_TLS` env var; added `eprintln!` logging throughout
- Added startup DNS+TCP connectivity test for huggingface.co
- Default models: 7B/30B → `stories15M` (0.008 GB) + `TinyLlama-1.1B-Chat` (0.65 GB) — prevents Linux OOM
- `current_model` default: `"ghostlink-30b-v1"` → `"stories15M"`

**Housekeeping:**
- `.gitignore` — added `models/` and `models.json`
- Documentation consolidated — session artifacts moved to `docs/archive/`

## Build Verification
- `npx vitest run` — **23/23 passed**
- `npx tsc --noEmit` — **OK**
- `cargo check` — **OK**

## Known Issues
1. **`Qwen3.5-4B-BF16.gguf` is corrupt** — renamed to `.bak`
2. **`/api/metrics` hangs on simulated backend** — no GPU present
3. **Worker networking is local-only** — discovery only detects same LAN
4. **No auth enforcement** — discovery token configured but not enforced
5. **Linux OOM with large models** — default models now tiny (15M/1.1B); avoid loading 7B+ without sufficient RAM
6. **Download from HuggingFace may fail** — if `GHOSTLINK_INSECURE_TLS=1` helps, TLS cert bundle may need updating

## To Restart
```powershell
cd C:\Users\rwill\Ghostlink
launch-complete.bat
```

For Linux:
```bash
cd ~/Ghostlink
bash launch-complete.sh
```

## Models on Disk
- `models/stories15M-q4_0.gguf` (~19 MB)
- `models/tinyllama-1.1b-chat-v1.0.Q2_K.gguf` (~483 MB)
