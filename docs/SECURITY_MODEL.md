# Security Model

This document summarizes current security assumptions for Ghost-Link runtime and GUI integrations, along with recommended production hardening.

## Scope

- Discovery and node coordination traffic.
- Runtime inter-node transport behavior.
- GUI/API interaction path for operator workflows.
- RPC peer authentication for distributed inference.
- Audit logging and observability.

## Current Controls

- **Supervised RPC Contributor Process & Revocation on Crash**: `ggml-rpc-server` is actively supervised by `rpc_cluster::RpcSupervisor`. Process health checks verify both PID status and TCP port responsiveness. If `ggml-rpc-server` crashes or freezes, Ghostlink immediately revokes its `contribute_compute` discovery advertisements (UDP/mDNS) and flags the node as unroutable (`excluded_reason: "rpc child not running"`) until auto-restart with exponential backoff successfully restores process and port health.
- Versioned discovery-frame authentication using HMAC-SHA256 with timestamp and nonce replay guards.
- Optional transport auth token controls for TCP flow runs.
- GUI readiness diagnostics and environment preflight checks.
- **Role-based API key access control** (since 2.0.0, `crates/ghost-link/src/auth.rs`):
  a persisted, hashed multi-key store (`api_keys.json` — SHA-256 hash + last-4
  preview only, the raw key value is never stored) replaces a single shared
  bearer token. Each key carries a role — `Admin`, `Operator`, or `Viewer` —
  and every route is gated accordingly: reads default to `Viewer`, mutating
  requests (POST/PUT/DELETE) default to `Operator`, and key management
  (`GET`/`POST /api/security/keys`, `DELETE /api/security/keys/:id`) plus
  `POST /api/security/pqc/enable` are `Admin`-only. The store refuses to
  delete the last remaining `Admin` key, preventing accidental lockout. An
  existing pre-2.0.0 `api_key.txt` migrates automatically on first run into a
  sole `bootstrap` Admin key — no manual step, and access is unchanged for a
  default single-key deployment. JWTs sign with a dedicated
  `jwt_signing_secret` (`jwt_secret.txt`, `GHOSTLINK_JWT_SECRET_PATH`
  override) rather than the raw API key, and a JWT is only honored while its
  subject key id is still present in the store — revoking a key immediately
  invalidates any outstanding JWT for it. *Note: this provides role-based API key authorization for operator access control (`Admin`, `Operator`, `Viewer`), but does not provide full multi-user / multi-tenant RBAC (user identities, team/project scoping, per-resource permissions).* See [API_REFERENCE.md](API_REFERENCE.md).
- **Real bearer-token auth on every API route but `/health`** (since 1.11.0):
  a 256-bit API key generated once on first run, or a short-lived JWT
  (`jsonwebtoken`, HS256) exchanged for it via `POST /api/security/jwt/refresh`.
  See [API_REFERENCE.md](API_REFERENCE.md).
- **Optional HTTPS with a genuine PQC-hybrid (X25519MLKEM768) key exchange**
  via `rustls`'s `prefer-post-quantum` feature — opt-in for plain-localhost
  dev, forced on when the server binds a non-loopback address. Off by
  default; enable via `POST /api/security/pqc/enable` (takes effect on next
  restart) and confirm with `GET /api/security/pqc/state`.
- **RPC peer authentication via `rpc_shared_secret` handshake** (since 2.0.0,
  `crates/ghost-link/src/rpc_cluster.rs`): the existing `rpc_allowed_peers` IP
  allowlist (1.17.0) doesn't stop a device already inside the allowed range,
  or one able to spoof a source address. When `rpc_shared_secret` is set, a
  dedicated auth port challenges a connecting peer with a random nonce; the
  peer must return `HMAC-SHA256(rpc_shared_secret, nonce)` to receive a
  time-limited admission for its source IP, which the allowlist proxy then
  requires in addition to plain IP membership. A fresh nonce per handshake
  defeats replay. **Off by default** — distributing the secret across a
  cluster's nodes is a manual, opt-in step. This does **not** encrypt the RPC
  byte stream itself (upstream llama.cpp's `--rpc` client leaves no protocol
  slot for that); it is a peer-admission control, not transport encryption.
- **Durable, append-only audit trail with CEF/JSON export** (since 2.0.0,
  `crates/ghost-link/src/audit_log.rs`): every audit event (auth failures,
  JWT refresh, PQC enable, key management, tool-call approve/deny) is written
  as a JSON line to `audit_log.jsonl` (`GHOSTLINK_AUDIT_LOG_PATH` override),
  in addition to the capped in-memory feed the GUI's Security tab reads live.
  `GET /api/security/audit-log/export?format=json|cef` returns the full
  durable history in JSON or Common Event Format for SIEM ingestion, and is
  gated `Admin`-only (the live capped feed remains `Viewer`-accessible). The
  durable file has **no built-in cap, rotation, or retention policy** — plan
  disk monitoring and log rotation for long-running deployments.
- **Opt-in OpenTelemetry tracing export** (since 2.0.0, `crates/ghost-link/src/otel.rs`):
  gated entirely on `GHOSTLINK_OTEL_EXPORTER_ENDPOINT` — unset, behavior is
  unchanged from prior releases. When set, HTTP requests and the distributed-
  inference path (peer discovery/admission, model load, generation) emit
  spans to any OTLP-compatible collector. `GHOSTLINK_OTEL_SERVICE_NAME`
  overrides the reported service name. Same protocol limitation as RPC
  auth above: a trace cannot span the actual `--rpc` hop itself.
- **Grafana + Prometheus monitoring profile** (since 2.0.0, opt-in via
  `docker compose up --profile monitoring`): scrapes the existing `/metrics`
  endpoint (bearer-token authenticated, like every route but `/health`).
  Change the default Grafana admin password (`GRAFANA_ADMIN_PASSWORD`)
  before running this profile beyond local evaluation.

## Threats and Risks

- Discovery spoofing or replay on untrusted LAN segments.
- Token leakage or weak token management for authenticated transport.
- MITM/tampering on networks where integrity and confidentiality controls are insufficient.
- Environment-dependent performance baselines causing noisy deployment decisions.

## Production Recommendations

1. Network trust boundaries:
- Treat discovery traffic as trusted-LAN only unless additional protections are added.
- Restrict broadcast/multicast scope via network segmentation.

2. Credential hygiene:
- Use strong, rotated auth tokens from secret managers.
- Do not hardcode tokens in scripts, configs, or container images.

3. Transport protection:
- Add optional mTLS for inter-node comms where confidentiality/integrity are required.
- The API server's own PQC-hybrid TLS (see Current Controls above) is real
  and available today; mTLS between fabric nodes themselves is still the
  open item.

4. Observability and audit:
- Log auth failures, discovery drop reasons, and repeated malformed frames.
- Keep audit logs immutable where possible and monitor for abuse patterns.

5. Baseline governance:
- Use relative drift and canary thresholds to reduce hardware variance noise.
- Prefer pinned runner classes or rolling baseline strategies for CI perf gates.

## Non-Goals (Current)

- Internet-exposed, zero-trust-ready deployment by default.
- Full zero-trust discovery posture by default (legacy CRC32 compatibility mode still exists for staged migration only).

## Roadmap Notes

Future security milestones should include:

- Optional mTLS mode in *fabric/runtime* transport (node-to-node), distinct
  from the API server's TLS above. The `rpc_shared_secret` handshake (2.0.0)
  authenticates RPC peer admission but does not encrypt the RPC stream
  itself — this remains the open gap.
- Enforced deprecation timeline for legacy CRC32 compatibility mode.
- Formal threat model review cadence tied to release checkpoints.
- Retention/rotation policy for the durable audit trail (`audit_log.jsonl`)
  — it is unbounded and append-only today.
