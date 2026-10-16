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
  bearer token. Each key carries a role — `owner`, `operator`, `inference`, or `viewer` —
  and every route is gated accordingly: reads default to `Viewer`, mutating
  requests (POST/PUT/DELETE) default to `Operator`, and key management
  (`GET`/`POST /api/security/keys`, `DELETE /api/security/keys/:id`) plus
  `POST /api/security/pqc/enable` are `owner`-only. The store refuses to
  delete the last remaining `Admin` key, preventing accidental lockout. An
  existing pre-2.0.0 `api_key.txt` migrates automatically on first run into a
  sole `bootstrap` Admin key — no manual step, and access is unchanged for a
  default single-key deployment. JWTs sign with a dedicated
  `jwt_signing_secret` (`jwt_secret.txt`, `GHOSTLINK_JWT_SECRET_PATH`
  override) rather than the raw API key, and a JWT is only honored while its
  subject key id is still present in the store — revoking a key immediately
  invalidates any outstanding JWT for it. *Note: this provides role-based API key authorization for operator access control (`owner`, `operator`, `inference`, `viewer`), but does not provide full multi-user / multi-tenant RBAC (user identities, team/project scoping, per-resource permissions).* See [API_REFERENCE.md](API_REFERENCE.md).
- **Real bearer-token auth on every API route but `/health`** (since 1.11.0):
  a 256-bit API key generated once on first run, or a short-lived JWT
  (`jsonwebtoken`, HS256) exchanged for it via `POST /api/security/jwt/refresh`.
  See [API_REFERENCE.md](API_REFERENCE.md).
- **Optional HTTPS with a genuine PQC-hybrid (X25519MLKEM768) key exchange**
  via `rustls`'s `prefer-post-quantum` feature — opt-in for plain-localhost
  dev, forced on when the server binds a non-loopback address. Off by
  default; enable via `POST /api/security/pqc/enable` (takes effect on next
  restart) and confirm with `GET /api/security/pqc/state`.
- **Encrypted RPC transport** (`rpc_tls`): a dual TCP proxy that terminates
  TLS on the contributor side and forwards decrypted traffic to the local
  `ggml-rpc-server` over plaintext loopback. Uses the same PQC-hybrid key
  exchange as the HTTP API TLS. Opt-in via `enable_rpc_tls` setting; when
  enabled and the bind address is non-loopback, the contributor runs a TLS
  terminator and the coordinator connects to the TLS port instead of the raw
  ggml-rpc port. Plaintext connections are rejected during the TLS handshake.
  This closes the gap where `ggml-rpc`'s byte stream was previously plain TCP.
- **ggml-rpc Build Fingerprint Matching & Default Unknown Exclusion**:
  `rpc_cluster::discover_rpc_peers` and `evaluate_peer` enforce matching `llama.cpp`
  build fingerprints (`rpc_build_id`). By default, peers with unknown or missing build
  fingerprints are excluded (`excluded_reason: "RPC build fingerprint missing"`) alongside
  explicit build mismatches (`excluded_reason: "RPC build does not match coordinator"`).
  For legacy mixed-version lab environments where unknown build IDs must be permitted,
  `rpc_allow_unknown_build_id = true` in settings or `GHOSTLINK_RPC_ALLOW_UNKNOWN_BUILD_ID=1`
  re-enables admission of unknown build fingerprints.
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
- **Durable, bounded audit trail with CEF/JSON export and rotation** (since 2.0.0,
  `crates/ghost-link/src/audit_log.rs`): every audit event (auth failures,
  JWT refresh, PQC enable, key management, tool-call approve/deny) is written
  as a JSON line to `audit_log.jsonl` (`GHOSTLINK_AUDIT_LOG_PATH` override),
  in addition to the capped in-memory feed the GUI's Security tab reads live.
  `GET /api/security/audit-log/export?format=json|cef` returns the full
  retained history across active and rotated files in JSON or Common Event Format
  for SIEM ingestion, and is gated `owner`-only (the live capped feed remains
  `Viewer`-accessible). Active audit log files are automatically capped and rotated
  (`audit_log.jsonl.1` .. `.N`) based on byte size (`GHOSTLINK_AUDIT_LOG_MAX_BYTES`, default 10MB)
  or line count (`GHOSTLINK_AUDIT_LOG_MAX_LINES`, default disabled), and retained up to a
  configurable file limit (`GHOSTLINK_AUDIT_LOG_MAX_FILES`, default 5 files) before purging older archives.
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

- **Server-side tool capability classification** (since 2.4.0, `crates/ghost-link/src/capability.rs`):
  every MCP tool call in the chat tool loop is classified `read` / `write` / `exec`
  in Rust, from a `(server, tool)` table, and the classification is enforced
  *before* dispatch in `invoke_mcp_tool`. This closes a gap the previous
  per-server `requires_confirmation` flag could not: that flag is per-server, so it
  could not distinguish `read_text_file` from `write_file` on the same filesystem
  server, and it could not express "this specific tool is vetted to auto-apply".
  **Classification fails closed** — any tool not in the table is treated as `exec`,
  including an unlisted tool on an otherwise-known server. The rationale is
  asymmetric cost: a wrong `read` permits arbitrary command execution, while a
  spurious `exec` costs one approval prompt.
  A system-prompt instruction to the model is explicitly *not* part of this
  control. The model is not the trust boundary; the server is.
  Session-scoped grants ("approve for session") are keyed by
  `(workspace_id, server, tool)` and **never cover `exec`** — a standing grant must
  not become a standing shell. The grant API refuses an `exec` class outright
  rather than relying on the caller to check, and `POST
  /api/inference/chat/tool-confirm` derives the class server-side instead of
  accepting one from the request body.
  The vetted auto-apply path admits a `write` only when its target canonicalizes
  inside the configured workspace root; `exec` and `read` are never auto-applied,
  and a write with no resolvable path is refused rather than assumed safe.
  `GET /api/inference/capabilities` exposes the effective classification of every
  connected tool so the boundary can be audited rather than inferred.
- **Per-workspace scoping of grants and data** (since 2.4.0, `crates/ghost-link/src/workspace.rs`):
  tool grants, memories, RAG indexes, and schedules are scoped by
  `workspace_id`, so a chat bound to workspace A cannot reach workspace B's files
  or data. A client-supplied `workspace_id` selects *which* workspace's data
  applies and never selects a filesystem root — the root remains
  server-configured (`GHOSTLINK_WORKSPACE_ROOT`), because accepting a
  caller-chosen root would hand out path traversal for free. Ids are sanitized and
  length-bounded before use as a filename or database key. Path containment uses a
  single shared `resolve_within` implementation, so the GUI's file routes and the
  tool caller's scoping cannot drift into two different traversal checks.

- **Workspace scope on memory tools is stamped server-side** (since 2.4.0, `crates/mcp-memory/`, `crates/ghost-link/src/capability.rs`):
  the memory store filters every statement by `workspace_id`, and `ghost-link`
  *overwrites* that argument at dispatch with the chat's own binding rather than
  accepting the model's. A prompt-injected turn can trivially emit
  `{"workspace_id": "ws_other"}`; that value is discarded, so it cannot read or
  write another workspace's memories. An unrecognized `kind` or `source` in an
  existing row degrades to a safe default rather than dropping the row, and
  `memory_forget` deletes scoped to the workspace, so an id carried over from
  elsewhere is a no-op instead of a cross-workspace delete.
  The store holds memory bodies and titles. It does not hold credentials: API
  keys remain in the hashed `api_keys.json` store and the OS keychain, and
  secrets are never written to the memory DB, to prompts, or to traces.

- **Non-blocking approval queue for gated tool calls** (since 2.4.0, `crates/ghost-link/src/approvals.rs`):
  `write` and `exec` calls are recorded as pending actions and surfaced to a
  review tray rather than stalling the chat turn. Three properties matter:
  the queued action's capability class is **re-derived server-side** from the
  classification table on the approve path, so a hand-edited `approvals.json`
  cannot promote a tool to a weaker class; `approve_for_session` refuses `exec`
  outright, so a standing grant cannot become a standing shell; and
  resolution is scoped to the owning `workspace_id`, returning an
  indistinguishable "not found" for another workspace's ids so the queue
  cannot be used to probe them.
  The queue stores full arguments (an approved action must actually run) but
  only a bounded **preview** leaves the process: `build_preview` reads specific
  named argument fields per server and has no generic fallback that would dump
  the argument bag, because that is where a credential would appear. The
  list endpoint returns `ActionSummary`, a type with no `args` field at all.
  A queued write is treated as a promise to the user and persisted, so it
  survives a restart rather than silently disappearing; a corrupt queue file
  degrades to an empty tray and is left on disk for inspection rather than
  overwritten. Resolved entries are pruned after 30 days
  (`GHOSTLINK_APPROVAL_MAX_AGE_DAYS`); pending entries are never pruned at any
  age. Tool reachability is checked *before* the gate, so an action queued
  against a disconnected server cannot exist — it would be unactionable.

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
