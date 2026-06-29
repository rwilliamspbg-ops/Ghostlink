# Production Readiness Review

## Scope

This checklist covers runtime reliability, CI gate coverage, GUI readiness, and operational hygiene for Ghost-Link.

## Current Status (2026-06-29)

- Rust workspace build/test/lint gates are configured and exercised in CI.
- Runtime smoke + SLO validation gates are enforced via `production-gate.yml`.
- GUI launch/readiness/diagnostics and mock backend contract checks are now validated.
- Coverage artifact generation is configured in CI.

## Release Gates

### Required (Hard Gates)

1. `CI` workflow green on target branch.
2. `Production Gate` workflow green on target branch.
3. `Lint` workflow green on target branch.
4. `Tests` workflow green on target branch.
5. GUI checks pass when GUI code changes:
   - `ghost-link gui-check --strict`
   - `ghost-link gui-diagnose --strict`
   - `third_party/mohawk_gui/test_dashboard.py` (headless mode in CI/devcontainer)

### Recommended (Operational)

1. Perf drift checks pass against current baseline files.
2. Stage-tail and canary guardrails pass for deterministic/stress snapshots.
3. Hugging Face verification script succeeds when model/bootstrap paths change.

## Command Set

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
python3 scripts/validate_gui_api_contract.py
python3 scripts/verify_hf_models.py
```

## Known Gaps

- AF_XDP/eBPF paths are still primarily unit-tested and Linux-specific.
- Full hardware probing depth depends on host tooling (`nvidia-smi`, `lspci`) availability.
- GUI currently relies on a Python runtime and desktop dependencies; packaging remains optional.

## Upgrade Backlog

1. Add a dedicated CI job for GUI function-matrix execution in headless mode.
2. Introduce signed release artifacts for Rust binaries and optional GUI bundle.
3. Add explicit secret/key material scanning to CI pre-merge checks.
4. Publish SLO dashboards from uploaded metrics artifacts.
