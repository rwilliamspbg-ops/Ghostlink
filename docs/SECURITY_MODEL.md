# Security Model

This document summarizes current security assumptions for Ghost-Link runtime and GUI integrations, along with recommended production hardening.

## Scope

- Discovery and node coordination traffic.
- Runtime inter-node transport behavior.
- GUI/API interaction path for operator workflows.

## Current Controls

- Versioned discovery-frame authentication using HMAC-SHA256 with timestamp and nonce replay guards.
- Optional transport auth token controls for TCP flow runs.
- GUI readiness diagnostics and environment preflight checks.
- **Real bearer-token auth on every API route but `/health`** (since 1.11.0):
  a 256-bit API key generated once on first run, or a short-lived JWT
  (`jsonwebtoken`, HS256) exchanged for it via `POST /api/security/jwt/refresh`.
  See [API_REFERENCE.md](API_REFERENCE.md).
- **Optional HTTPS with a genuine PQC-hybrid (X25519MLKEM768) key exchange**
  via `rustls`'s `prefer-post-quantum` feature — opt-in for plain-localhost
  dev, forced on when the server binds a non-loopback address. Off by
  default; enable via `POST /api/security/pqc/enable` (takes effect on next
  restart) and confirm with `GET /api/security/pqc/state`.

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
  from the API server's TLS above.
- Enforced deprecation timeline for legacy CRC32 compatibility mode.
- Formal threat model review cadence tied to release checkpoints.
- `GET /api/security/audit-log` is currently a stub that always returns an
  empty list — wiring it to actually record auth failures and other events
  is still open.
