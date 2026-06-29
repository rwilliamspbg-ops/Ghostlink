# Ghostlink Studio Execution Plan

## Workstreams

1. Security and release trust baseline
2. Core protocol and runtime readiness
3. Desktop app shell and UX delivery
4. Packaging, provenance, and rollout

## Sprint Plan

### Sprint 0: Prerequisites (Done in this branch)

- Secret hygiene hardening
- License consistency enforcement
- CI/release strictness updates
- Discovery auth migration scaffolding

### Sprint 1: Studio Foundation

- Create Tauri app shell and command bridge
- Build sidebar layout and page routing
- Implement Home dashboard cards with mock metrics

### Sprint 2: Runtime Integration

- Wire doctor, probe, and flow commands to backend handlers
- Add Cluster view with live node summaries
- Add Settings page with TOML load/save support

### Sprint 3: User Workflows

- Build Models page with compatibility checks
- Build Chat page with streaming responses
- Add quick action panel and onboarding tour

### Sprint 4: Stabilization and Packaging

- Add installer builds for Linux/Windows
- Integrate release signing, SBOM, and attestations
- Add smoke tests for packaged app startup and diagnostics

## Acceptance Gates

- First-run to successful local chat in under 10 minutes
- Doctor diagnostics available from UI and exportable as JSON
- Signed artifacts and provenance generated for tagged releases
- No high/critical security findings in CI on release candidates
