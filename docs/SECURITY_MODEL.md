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
- Evaluate PQC/hybrid key exchange roadmap for long-lived deployments.

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

- Optional mTLS mode in runtime transport.
- Enforced deprecation timeline for legacy CRC32 compatibility mode.
- Formal threat model review cadence tied to release checkpoints.
