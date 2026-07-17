# Troubleshooting

## Settings tab doesn't load

**Cause:** `RuntimeDetector::detect()` calls `nvidia-smi` synchronously, blocking the async thread. Fixed in commit `e161ac1` by moving detection to `spawn_blocking` with 30s timeout.

**Fix:** Rebuild: `cargo build -p ghost-link` and restart.

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
