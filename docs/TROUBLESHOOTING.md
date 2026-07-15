# Troubleshooting

## Download from Hugging Face fails

**Symptom:** Clicking Download shows "Downloading" but no progress; model never appears.

**Causes & fixes:**

1. **Trailing space in `VITE_GHOSTLINK_API_BASE`** — The `.bat` file's inline `set` command had a space before `&&`. Ensure `launch-complete.bat` uses `set "VITE_GHOSTLINK_API_BASE=http://127.0.0.1:8003"` (quoted, no trailing space).

2. **TLS certificate issue (Windows)** — If the backend logs `[download_hf_model] API error for ...: ...tls...`, set:
   ```batch
   set GHOSTLINK_INSECURE_TLS=1
   ```
   Then re-run `launch-complete.bat`. If this fixes it, your system needs updated CA certificates.

3. **Proxy blocks outbound HTTPS** — The backend uses `.no_proxy()` to bypass system proxy. If behind a corporate proxy, try setting `GHOSTLINK_INSECURE_TLS=1` or configure a direct internet connection.

4. **Model not found** — Some HF repos have no GGUF files. Try `lmstudio-community/Meta-Llama-3-8B-Instruct-GGUF` or other GGUF-quantized variants.

## Settings tab doesn't load

**Cause:** `RuntimeDetector::detect()` calls `nvidia-smi` synchronously, blocking the async thread. Fixed in commit `e161ac1` by moving detection to `spawn_blocking` with 30s timeout.

**Fix:** Rebuild: `cargo build -p ghost-link` and restart.

## Linux out of memory (OOM) when loading a model

**Cause:** The default models were 7B–30B parameters, requiring 4–16 GB RAM.

**Fix:** Commit `e161ac1` replaced defaults with `stories15M` (15M params, ~8 MB) and `TinyLlama-1.1B-Chat`. Rebuild and restart.

If you still see OOM, don't load models larger than your available RAM.

## Library tab shows "Downloading" but no progress bar

**Cause:** A previous download attempt left a stale entry in `models.json`. The backend now cleans stale "Downloading" entries on startup.

**Fix:** Restart the backend. If the issue persists, delete `models.json` while the backend is stopped.

## Frontend says "Connection Error"

1. Check the backend is running: `curl http://127.0.0.1:8003/health`
2. Check `VITE_GHOSTLINK_API_BASE` is set correctly in the environment (should be `http://127.0.0.1:8003`)
3. Check the backend console for errors (Rust panic, port in use)

## Port already in use

Kill existing processes:
```powershell
taskkill /f /im ghost-link.exe && taskkill /f /im node.exe
```

## Logs

| Component | Location |
|---|---|
| Backend API | Console window (or `/tmp/ghostlink_api.log` on Linux) |
| Frontend | Browser dev console + Vite terminal |
| llama-server | `/tmp/ghostlink_llama_server.log` (Linux) |
| HF download | Backend console — look for `[download_hf_model]` entries |
