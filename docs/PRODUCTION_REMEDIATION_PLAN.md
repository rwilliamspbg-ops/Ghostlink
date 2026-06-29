# Production Remediation Plan

This plan converts the current "advanced prototype" gaps into release-ready milestones.
It is aligned to the current readiness checklist in `PRODUCTION_READINESS.md` and the security posture in `SECURITY_MODEL.md`.

## Goals

1. Make multi-node LAN behavior reliable under real faults.
2. Raise security posture from trusted-LAN to optional zero-trust mode.
3. Close feature gaps called out in docs (dynamic rebalance and migration).
4. Improve operator confidence with stronger monitoring and packaging.

## Baseline Assumptions (Current)

- Core CI and production gate are already active.
- Runtime smoke, SLO checks, perf drift, and stage-tail/canary guardrails are active.
- GUI launch and diagnostics checks are active, but GUI packaging is optional.

## Phase 0: Gap Triage and Tracking (Week 1)

### Deliverables

- Create a GitHub project board with tracks:
  - Security
  - Reliability
  - Networking
  - Observability
  - Packaging
- Open one issue per work item in this plan and label by phase.
- Add a weekly status template to PR descriptions for this branch family.

### Phase 0 Exit Criteria

- Every item below exists as a tracked issue with owner and target sprint.
- All production-risk issues are tagged `risk:prod`.

## Phase 1: Security Hardening (Weeks 1-3)

### Phase 1 Work Items

1. Transport security mode:
   - Add optional mTLS for inter-node runtime transport.
   - Support cert chain validation and hostname/SAN checks.
2. Discovery auth hardening:
   - Add authenticated discovery frames (HMAC with rotating key ID).
   - Reject unauthenticated discovery when secure mode is enabled.
3. Secret management:
   - Move token/cert loading to file/env providers with redaction in logs.
4. CI security checks:
   - Add secret scanning and dependency advisory checks in CI.

### Phase 1 Exit Criteria

- `ghost-link` supports `--security-mode trusted-lan|mtls`.
- Secure mode integration test suite passes (auth success + auth failure paths).
- CI blocks merges on secret scan failures.

## Phase 2: Multi-Node Reliability Validation (Weeks 2-5)

### Phase 2 Work Items

1. Real-LAN validation matrix:
   - Run 2-node, 4-node, and 8-node scenarios on real hardware.
   - Validate discovery convergence, steady-state throughput, and failover.
2. Fault injection harness:
   - Add packet loss, node restart, and temporary partition scenarios.
3. Recovery semantics:
   - Define and implement bounded retry/backoff + rejoin behavior.

### Phase 2 Exit Criteria

- Publish reproducible LAN test report in CI artifacts.
- Meet SLO thresholds for failover and recovery time under defined fault cases.
- No critical regressions across 7 consecutive daily runs.

## Phase 3: Feature Completeness (Weeks 4-7)

### Phase 3 Work Items

1. Dynamic rebalancing:
   - Implement runtime rebalance triggers based on node health/load drift.
2. Tensor migration path:
   - Add migration planner and safe handoff sequence for in-flight work.
3. Consistency safeguards:
   - Add rollback path when migration or rebalance exceeds tail-latency budget.

### Phase 3 Exit Criteria

- Rebalance and migration e2e tests pass.
- Throughput regression <= 10% and p95 regression <= 15% during controlled rebalance.
- Feature flags allow staged rollout and quick disable.

## Phase 4: Networking and AF_XDP Readiness (Weeks 5-8)

### Phase 4 Work Items

1. AF_XDP compatibility matrix:
   - Kernel/NIC matrix documentation and test coverage.
2. Driver-path verification:
   - Add integration tests for AF_XDP fallback to standard transport.
3. Linux packaging docs:
   - Provide minimal host requirements and preflight checker output.

### Phase 4 Exit Criteria

- AF_XDP mode passes supported matrix in CI or nightly hardware lane.
- Fallback behavior is deterministic and tested.

## Phase 5: Monitoring and Operations (Weeks 6-9)

### Phase 5 Work Items

1. Active network probing:
   - Add optional ping/RTT probe module (ICMP or TCP probe fallback).
2. Alerting signals:
   - Define alert thresholds for heartbeat gaps, jitter, and sustained retries.
3. Dashboard/API surfacing:
   - Expose probe health and historical trend windows in CLI/GUI output.

### Phase 5 Exit Criteria

- Operators can distinguish host overload vs network path issues.
- Alert playbooks documented and tested in fault simulations.

## Phase 6: Packaging and Release Discipline (Weeks 8-10)

### Phase 6 Work Items

1. Release artifacts:
   - Produce signed binaries (and optional GUI bundle) per release.
2. Install experience:
   - Add reproducible install script and checksum verification.
3. Release checklist:
   - Tie `PRODUCTION_READINESS.md` gates to release tag process.

### Phase 6 Exit Criteria

- Signed artifacts and checksums are published for each release candidate.
- Release process is documented and repeatable by a second maintainer.

## Metrics and Governance

Track these weekly:

- Security: count of unresolved `risk:prod` security issues.
- Reliability: pass rate of LAN/fault-injection matrix.
- Performance: drift ratio vs baseline in deterministic/stress profiles.
- Operations: mean time to detect and classify node/network faults.

## Suggested Order of Execution

1. Complete Phase 0 immediately.
2. Run Phases 1 and 2 in parallel (security + reliability).
3. Start Phase 3 once reliability baselines are stable.
4. Deliver Phase 4 and 5 before first production candidate tag.
5. Gate Phase 6 on all prior exit criteria.

## Definition of Production-Ready (Target)

Ghost-Link is considered production-ready when:

- Secure mode (mTLS + authenticated discovery) is available and tested.
- Multi-node LAN fault-injection suite is green and stable.
- Dynamic rebalance and migration are feature-complete and guarded.
- AF_XDP behavior is documented, tested, and safely fallbacks.
- Monitoring includes active network probes with actionable alerts.
- Release artifacts are signed and repeatably published.
