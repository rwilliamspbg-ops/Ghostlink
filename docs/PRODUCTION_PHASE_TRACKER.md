# Production Phase Tracker

This tracker turns the remediation plan into a work queue that can be copied into issues and PRs.
It is aligned to [docs/PRODUCTION_REMEDIATION_PLAN.md](docs/PRODUCTION_REMEDIATION_PLAN.md).

## Usage

1. Open one issue per unchecked item using the `Production Phase Task` issue template.
   Seed drafts for the first P0 batch live in [docs/PRODUCTION_ISSUE_SEEDS.md](docs/PRODUCTION_ISSUE_SEEDS.md).
2. Assign an owner, target sprint, and board track.
3. Link the issue in the matching tracker row.
4. Update PRs with weekly status using `.github/pull_request_template.md`.

## Phase 0: Triage and Tracking

- [ ] Create GitHub project board tracks: Security, Reliability, Networking, Observability, GUI / UX, Packaging, Hardware Matrix.
- [ ] Open one issue per work item below and label with `risk:prod` where applicable.
- [ ] Link all created issues back into this tracker.
- [ ] Require weekly PR status updates for remediation work.

## Phase 1: Security Hardening

| Item | Track | Priority | Issue | Exit signal |
| --- | --- | --- | --- | --- |
| Optional mTLS runtime transport mode | Security | P0 | TODO | `--security-mode` supports `trusted-lan` and `mtls`, and both are tested |
| Certificate validation and SAN/hostname checks | Security | P0 | TODO | Success/failure integration coverage exists |
| Authenticated discovery with rotating key ID | Security | P0 | TODO | Secure-mode discovery rejects unauthenticated frames |
| Token/cert loading providers with log redaction | Security | P1 | TODO | Secrets not exposed in logs or examples |
| CI secret scanning and advisory enforcement | Security | P0 | TODO | CI blocks merges on security scan failures |

## Phase 2: Multi-Node Reliability Validation

| Item | Track | Priority | Issue | Exit signal |
| --- | --- | --- | --- | --- |
| Real-LAN 2/4/8-node validation matrix | Reliability | P0 | TODO | Reproducible LAN report published |
| Heterogeneous host coverage beyond primary dev setup | Hardware Matrix | P0 | TODO | Coverage includes mixed CPU/GPU/NIC classes |
| Packet-loss, restart, and partition scenarios | Reliability | P0 | TODO | Fault matrix covers all three cases |
| Long-duration soak with join/leave churn | Reliability | P1 | TODO | 7 daily runs without critical regression |
| Bounded retry/backoff and rejoin semantics | Reliability | P0 | TODO | Recovery SLOs are measured and green |

## Phase 3: Feature Completeness

| Item | Track | Priority | Issue | Exit signal |
| --- | --- | --- | --- | --- |
| Runtime rebalance triggers based on health/load drift | Reliability | P0 | TODO | Rebalance e2e suite passes |
| Tensor migration planner and in-flight handoff | Reliability | P0 | TODO | Migration e2e suite passes |
| Rollback path when tail budget is exceeded | Reliability | P1 | TODO | Feature flag and disable path validated |
| Controlled rebalance perf budget validation | Observability | P1 | TODO | Throughput <= 10% drop and p95 <= 15% rise |

## Phase 4: Networking and AF_XDP Readiness

| Item | Track | Priority | Issue | Exit signal |
| --- | --- | --- | --- | --- |
| AF_XDP kernel/NIC compatibility matrix | Networking | P0 | TODO | Supported matrix is green in CI or nightly hardware lane |
| AF_XDP fallback integration coverage | Networking | P0 | TODO | Fallback behavior is deterministic and tested |
| Minimal Linux host requirements docs | Packaging | P1 | TODO | Preflight guidance is documented |
| Tool-independent hardware probe fallback | Networking | P1 | TODO | Probe output stays actionable without `nvidia-smi`/`lspci` |
| Explicit degraded-mode probe reasons | Observability | P1 | TODO | Operators can see why probe depth was reduced |

## Phase 5: Monitoring and Operations

| Item | Track | Priority | Issue | Exit signal |
| --- | --- | --- | --- | --- |
| Active ICMP/TCP path probe module | Observability | P0 | TODO | Operators can separate path issues from host overload |
| Alert thresholds for heartbeat gaps, jitter, retries | Observability | P1 | TODO | Alert playbooks documented and exercised |
| CLI/GUI surfacing of probe health and trend windows | GUI / UX | P1 | TODO | Probe health is visible in operator surfaces |
| Fault classification workflow and runbook updates | Observability | P1 | TODO | Runbooks cover detection and first response |

## Phase 6: Packaging and Release Discipline

| Item | Track | Priority | Issue | Exit signal |
| --- | --- | --- | --- | --- |
| Signed runtime binaries and checksums | Packaging | P0 | TODO | Release candidates publish signed artifacts |
| Reproducible install path with verification | Packaging | P1 | TODO | Second maintainer can reproduce install flow |
| Release checklist tied to readiness gates | Packaging | P0 | TODO | Tag process blocks on readiness checklist |
| Headless GUI function-matrix CI lane | GUI / UX | P0 | DONE | `production-gate.yml` runs xvfb dashboard smoke |
| Expand GUI automation to additional platform lanes | GUI / UX | P1 | TODO | GUI automation is green across supported platform lanes |
| Standalone GUI bundle for supported targets | GUI / UX | P1 | TODO | GUI bundle ships with release candidate |

## Weekly Review

- Security unresolved `risk:prod` count:
- Reliability matrix pass rate:
- Deterministic/stress drift status:
- Mean time to classify host vs network faults:
- GUI automation lane status:
- Hardware coverage count:
