# Production Issue Seeds

These drafts are ready to copy into GitHub issues using the `Production Phase Task` template.
They cover the first batch of P0 remediation work across all phases.

## Seed 1: Optional mTLS Runtime Transport Mode

- Phase: Phase 1: Security Hardening
- Target sprint: Sprint 32 / RC-1
- Track: Security
- Owner: @unassigned

Problem statement:
Ghost-Link currently assumes a trusted LAN plus token-based controls for runtime transport. That is insufficient for deployments that require confidentiality and authenticated peer identity. The production plan requires an optional mTLS mode so inter-node runtime transport can operate beyond the current trusted-LAN posture.

Implementation scope:
- Add a transport security mode selection surface such as `--security-mode trusted-lan|mtls`.
- Implement certificate/key loading for the runtime transport path.
- Enforce certificate chain validation plus SAN/hostname validation in mTLS mode.
- Add integration coverage for successful secure handshakes and expected rejection paths.
- Update operator docs and rollout guidance.

Dependencies / prerequisites:
- Depends on: final runtime transport interface for secure mode wiring.
- Requires: local test certificates and CI-safe fixture generation.
- External blockers: none.

Exit criteria:
- [ ] Runtime transport supports `trusted-lan` and `mtls` modes.
- [ ] Authenticated secure transport succeeds with valid certs.
- [ ] Invalid cert, unknown CA, and hostname mismatch paths fail closed.
- [ ] Docs explain secure mode enablement and rollback path.

Validation and artifacts:
- Commands:
  - `cargo test --workspace`
  - secure-mode integration suite
- CI workflows:
  - `security.yml`
  - `production-gate.yml`
- Artifact/report links:
  - integration test logs
  - sample secure-mode configuration

Risk and rollout:
- [ ] Feature flag or rollback path defined
- [ ] Backward compatibility validated
- [ ] Security implications reviewed

## Seed 2: Real-LAN Validation Matrix

- Phase: Phase 2: Multi-Node Reliability Validation
- Target sprint: Sprint 32 / RC-1
- Track: Reliability
- Owner: @unassigned

Problem statement:
Ghost-Link is not broadly qualified for production LAN deployment because most runtime validation is still local or synthetic. We need repeatable real-LAN coverage across 2-node, 4-node, and 8-node scenarios to establish discovery convergence, steady-state throughput, and failover behavior.

Implementation scope:
- Define a reproducible LAN test matrix for 2/4/8-node runs.
- Capture discovery, throughput, failover, and recovery metrics for each topology.
- Store outputs as CI or nightly artifacts.
- Document the supported baseline hardware and topology assumptions.

Dependencies / prerequisites:
- Depends on: access to real multi-node lab hardware.
- Requires: artifact upload path and repeatable environment setup.
- External blockers: runner availability and lab scheduling.

Exit criteria:
- [ ] Reproducible LAN matrix exists for 2/4/8-node runs.
- [ ] Discovery convergence and failover metrics are captured.
- [ ] Nightly or lab report artifacts are published.
- [ ] No critical regressions across 7 consecutive daily runs.

Validation and artifacts:
- Commands:
  - phase-specific LAN validation runner
- CI workflows:
  - nightly or hardware-lab lane
- Artifact/report links:
  - LAN summary report
  - metrics artifacts

Risk and rollout:
- [ ] Feature flag or rollback path defined
- [ ] Backward compatibility validated
- [ ] Security implications reviewed

## Seed 3: Runtime Rebalance Triggers

- Phase: Phase 3: Feature Completeness
- Target sprint: Sprint 33 / RC-1
- Track: Reliability
- Owner: @unassigned

Problem statement:
Dynamic rebalancing remains early-stage, which leaves the runtime unable to react predictably to sustained load or health drift. The production plan requires guarded rebalance triggers and end-to-end validation for this path.

Implementation scope:
- Define health/load drift thresholds that trigger rebalance.
- Implement guarded runtime rebalance triggers with feature-flag control.
- Add rollback behavior when rebalance exceeds latency budget.
- Add e2e tests and performance-budget validation during controlled rebalance.

Dependencies / prerequisites:
- Depends on: reliable health and load signals from runtime metrics.
- Requires: benchmark or scenario harness that can trigger rebalance deterministically.
- External blockers: none.

Exit criteria:
- [ ] Rebalance triggers are implemented behind a controlled rollout path.
- [ ] Rebalance e2e tests pass.
- [ ] Controlled rebalance stays within throughput and p95 budgets.
- [ ] Fast disable path is documented.

Validation and artifacts:
- Commands:
  - rebalance e2e suite
  - controlled perf validation run
- CI workflows:
  - `production-gate.yml` or phase-specific extension
- Artifact/report links:
  - rebalance metrics snapshots
  - rollback exercise notes

Risk and rollout:
- [ ] Feature flag or rollback path defined
- [ ] Backward compatibility validated
- [ ] Security implications reviewed

## Seed 4: AF_XDP Kernel/NIC Compatibility Matrix

- Phase: Phase 4: Networking and AF_XDP Readiness
- Target sprint: Sprint 33 / RC-1
- Track: Networking
- Owner: @unassigned

Problem statement:
AF_XDP and eBPF readiness remains under-validated on real hardware. Current confidence is mostly unit-level and Linux-specific. We need an explicit kernel/NIC compatibility matrix and deterministic fallback validation.

Implementation scope:
- Define supported kernel/NIC combinations.
- Run AF_XDP preflight and runtime validation across those combinations.
- Add integration coverage for deterministic fallback to standard transport.
- Document supported and unsupported environments.

Dependencies / prerequisites:
- Depends on: hardware lab access or nightly runners with varied NIC/kernel combinations.
- Requires: preflight artifact capture and fallback test scenarios.
- External blockers: hardware lane capacity.

Exit criteria:
- [ ] Compatibility matrix is documented and versioned.
- [ ] Supported matrix entries pass in CI or nightly hardware lane.
- [ ] Fallback behavior is deterministic and tested.
- [ ] Operator docs describe supported environments and fallback expectations.

Validation and artifacts:
- Commands:
  - `python3 scripts/xdp_preflight_check.py --output ...`
  - AF_XDP integration runner
- CI workflows:
  - nightly hardware lane
- Artifact/report links:
  - compatibility matrix report
  - fallback test logs

Risk and rollout:
- [ ] Feature flag or rollback path defined
- [ ] Backward compatibility validated
- [ ] Security implications reviewed

## Seed 5: Active Network Path Probe Module

- Phase: Phase 5: Monitoring and Operations
- Target sprint: Sprint 33 / RC-1
- Track: Observability
- Owner: @unassigned

Problem statement:
Current health monitoring relies mainly on heartbeats and collected metrics. Operators still lack a first-class signal to distinguish node overload from network-path degradation. The production plan calls for active ICMP/TCP probes and surfaced path health.

Implementation scope:
- Add an active network path probe module with ICMP where available and TCP fallback otherwise.
- Record RTT, failures, and classification signals.
- Surface probe health in CLI/GUI and operator artifacts.
- Define alert thresholds and update runbooks.

Dependencies / prerequisites:
- Depends on: existing health and diagnostics command surfaces.
- Requires: environments where ICMP may be unavailable so TCP fallback is exercised.
- External blockers: none.

Exit criteria:
- [ ] Probe module supports ICMP or TCP fallback.
- [ ] Operators can distinguish host overload from network-path issues.
- [ ] Alert thresholds and runbooks are documented.
- [ ] Probe results are visible in CLI/GUI or exported artifacts.

Validation and artifacts:
- Commands:
  - `python3 scripts/active_network_probe.py --target ...`
  - updated doctor or diagnostics flows
- CI workflows:
  - `production-gate.yml`
- Artifact/report links:
  - probe summary artifacts
  - updated runbooks

Risk and rollout:
- [ ] Feature flag or rollback path defined
- [ ] Backward compatibility validated
- [ ] Security implications reviewed

## Seed 6: Headless GUI Function-Matrix Lane

- Phase: Phase 6: Packaging and Release Discipline
- Target sprint: Sprint 34 / RC-1
- Track: GUI / UX
- Owner: @unassigned

Problem statement:
GUI validation is still partly manual or devcontainer-based, which leaves packaging and release confidence lower than the runtime path. The production plan requires an automated headless GUI function-matrix lane before release candidates.

Implementation scope:
- Add a CI or nightly workflow that runs the GUI function matrix in headless mode.
- Ensure required Python and display dependencies are provisioned deterministically.
- Upload GUI validation artifacts for triage.
- Document when GUI changes must satisfy this lane.

Dependencies / prerequisites:
- Depends on: stable GUI test harness and headless display strategy.
- Requires: CI environment with xvfb or equivalent.
- External blockers: runner image/package availability.

Exit criteria:
- [ ] Headless GUI function-matrix workflow exists.
- [ ] Workflow is green on supported targets.
- [ ] GUI artifacts are uploaded for failures.
- [ ] Release guidance references the lane as a required check.

Validation and artifacts:
- Commands:
  - `cargo run -p ghost-link -- gui-check --strict`
  - `cargo run -p ghost-link -- gui-diagnose --strict`
  - `third_party/mohawk_gui/test_dashboard.py`
- CI workflows:
  - GUI nightly or release-adjacent lane
- Artifact/report links:
  - GUI matrix output
  - diagnostics artifacts

Risk and rollout:
- [ ] Feature flag or rollback path defined
- [ ] Backward compatibility validated
- [ ] Security implications reviewed
