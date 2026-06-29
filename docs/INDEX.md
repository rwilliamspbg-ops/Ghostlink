# Documentation Index

This index is the source of truth for active documentation in Ghost-Link.

## Active Docs

- [../README.md](../README.md): project overview, quick start, primary command reference, and doctor command.
- [QUICKSTART.md](QUICKSTART.md): copy/paste first-run path and common setup fixes.
- [../TESTING.md](../TESTING.md): validated commands, CI gate checks, and known gaps.
- [ARCHITECTURE.md](ARCHITECTURE.md): architecture and module responsibilities.
- [EXAMPLES.md](EXAMPLES.md): runnable CLI and API examples.
- [DEPLOYMENT.md](DEPLOYMENT.md): deployment path from single-node validation to staged LAN rollout.
- [MOHAWK_GUI.md](MOHAWK_GUI.md): GUI launch, diagnostics, MCP JSON usage, and troubleshooting.
- [SECURITY_MODEL.md](SECURITY_MODEL.md): threat model and security posture guidance.
- [SECURITY_SECRETS_REMEDIATION.md](SECURITY_SECRETS_REMEDIATION.md): credential rotation and git history cleanup procedure.
- [TROUBLESHOOTING.md](TROUBLESHOOTING.md): common failures and operational fixes.
- [PRODUCTION_READINESS.md](PRODUCTION_READINESS.md): production readiness checklist and status.
- [PRODUCTION_REMEDIATION_PLAN.md](PRODUCTION_REMEDIATION_PLAN.md): phased closure plan for known production gaps.
- [GHOSTLINK_STUDIO_PRODUCT_VISION.md](GHOSTLINK_STUDIO_PRODUCT_VISION.md): Ghostlink Studio UX and product scope.
- [GHOSTLINK_STUDIO_EXECUTION_PLAN.md](GHOSTLINK_STUDIO_EXECUTION_PLAN.md): sprint-by-sprint implementation roadmap.

## Baselines and Perf Policy

- [PERF_BASELINE.json](PERF_BASELINE.json): deterministic baseline and drift policy.
- [PERF_BASELINE_STRESS.json](PERF_BASELINE_STRESS.json): stress profile baseline and drift policy.

## Historical Docs

Historical status snapshots and one-off implementation summaries are archived at:

- [archive/INDEX.md](archive/INDEX.md)

## Documentation Policy

1. Keep active operational guidance in `README.md`, `TESTING.md`, or files under `docs/`.
2. When a status doc is superseded, move it to `docs/archive/` and update `docs/archive/INDEX.md`.
3. Avoid duplicate command references; link to canonical sections from one source.
