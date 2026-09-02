# Troubleshooting

## Settings tab doesn't load

**Cause:** `RuntimeDetector::detect()` calls `nvidia-smi` synchronously, blocking the async thread. Fixed in commit `e161ac1` by moving detection to `spawn_blocking` with 30s timeout.

**Fix:** Rebuild: `cargo build -p ghost-link` and restart.

## Frontend says "Connection Error"

1. Check the control-plane gateway health: `curl http://127.0.0.1:8000/health` (or internal API: `curl http://127.0.0.1:8003/health`)
2. Check `VITE_GHOSTLINK_API_BASE` is set correctly in the environment (should be `http://127.0.0.1:8000`)
3. Check the backend console for errors (Rust panic, port in use)

## Port already in use

Kill existing processes:
```powershell
taskkill /f /im ghost-link.exe && taskkill /f /im node.exe
```

## Distributed inference: a peer is discovered but never used

In Ghostlink Phase 4+, check the **Cluster Topology / Workers** tab in the GUI. Every discovered peer card explicitly displays its role, build ID match status, secret handshake status, allowlist status, and an exact exclusion reason banner if it is not used in the placement plan.

Common reasons and fixes:
- **rpc child not running:** The peer node's `ggml-rpc-server` child process crashed or failed to bind its port. Ghostlink's supervisor automatically attempts to restart the process with bounded exponential backoff (up to 10 restarts). Check `GHOSTLINK_RPC_SERVER_LOG` or peer system logs for crash details.
- **RPC build does not match coordinator:** The peer is running a different `llama.cpp` build commit. Rebuild both nodes at the same `llama.cpp` commit.
- **rpc_shared_secret missing or handshake mismatch:** The peer has a different or missing shared secret. Ensure identical secrets are set in Workers > Advanced.
- **peer IP not in coordinator rpc_allowed_peers:** The peer's IP address or CIDR range is blocked by the coordinator's allowlist. Add its IP to `rpc_allowed_peers`.
- **contribute_compute off / no rpc_port advertised:** The peer has not enabled "Contribute local GPU/CPU compute" or advertised an `rpc_port`.
- **unhealthy / stale heartbeat:** The peer's network connection or heartbeat has timed out.

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


### Metrics tokens look wrong or display 0 tok/s

- **Throughput Formula**: Ghostlink measures inference throughput as `throughput_tokens_per_sec = generated_tokens / generation_seconds` (where `generation_seconds = latency_ms / 1000.0`).
- **Exponential Moving Average (EMA)**: The dashboard "Throughput" gauge displays an EMA across active inference turns (`0.65` prior EMA + `0.35` new sample).
- **Zero-Token & Error Filtering**: Stream cancellations, failed requests, or 0-token generations are excluded from EMA calculations so they do not drag performance graphs down to zero.
- **Unit Reconciliation**: Both `/api/metrics` and `ChatTab` live stream `tok/s` use standard tokens-per-second (`tok/s`). Fabric node throughput graphs remain on internal GB/s or k-token scales without affecting client GUI metrics.

### Distributed Offload Fallback or No-Op Warning
- **Symptom:** Placement plan summary states `Single-machine inference on local node ... (Distributed offload warning: ...)` despite `distributed_inference` being enabled.
- **Cause 1:** `-ngl` is set to 0 (CPU-only), so no layers leave the coordinator.
- **Cause 2:** Remote tensor split share is below 1%, meaning remote peer VRAM/RAM is negligible relative to local node capacity.
- **Fix:** Set `-ngl` > 0 (or leave `ngl_auto: true`), and ensure remote peer nodes have sufficient VRAM/RAM for > 1% share. If strict cluster offload is required, set `GHOSTLINK_REQUIRE_CLUSTER_OFFLOAD=1` in the environment.

### Model-Ready Timeout During Cross-Machine Load
- **Symptom:** `llama-server` load times out waiting for `/health` during large model loads across multiple RPC nodes.
- **Cause:** Large GGUF file distribution and tensor initialization across slow network links exceeds the calculated timeout.
- **Fix:** Increase the timeout via `GHOSTLINK_MODEL_READY_TIMEOUT_SECS=1200` (or higher up to 1800s).
