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

## Distributed inference: a peer is discovered but never used

Check the peer's `rpc_build_id`. Since 1.17.0, the coordinator refuses to
route inference through an RPC peer running a different `llama.cpp` build —
a mismatch previously caused silent output corruption on larger models
rather than an obvious failure. Rebuild the peer to match the coordinator's
`llama.cpp` version and re-run discovery.

If peers are on 2.0.0+ and `rpc_shared_secret` is set in `ghostlink.toml`,
confirm the *same* secret is configured on every node — a peer with a
missing or mismatched secret fails the handshake and is silently excluded
from routing rather than erroring loudly. Peers below 2.0.0, or any node
with `rpc_shared_secret` unset, fall back to the plain `rpc_allowed_peers`
IP allowlist behavior.

## 401s after upgrading to 2.0.0

If a client cached a JWT issued before the 2.0.0 upgrade, it stops
validating after the process restarts — JWTs now sign with a dedicated
`jwt_signing_secret` instead of the raw API key. Re-authenticate (the
underlying API key itself still works and auto-migrated to an Admin-role
key; only cached short-lived JWTs are affected, and they expire within an
hour regardless).

## Logs

| Component | Location |
|---|---|
| Backend API | Console window (or `/tmp/ghostlink_api.log` on Linux) |
| Frontend | Browser dev console + Vite terminal |
