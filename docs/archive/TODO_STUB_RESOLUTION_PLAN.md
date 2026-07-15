# TODO Stub Resolution Plan

## Goal
Resolve all current `TODO: Implement actual ...` stubs in `crates/ghost-link/src` with production behavior, tests, and documentation.

## Current Stub Inventory
1. `crates/ghost-link/src/api/mod.rs` (`chat_completion`): actual Ollama connection.
2. `crates/ghost-link/src/cli/dashboard.rs` (`execute`): dashboard rendering.
3. `crates/ghost-link/src/cli/doctor.rs` (`execute`): diagnostics checks.
4. `crates/ghost-link/src/cli/flow.rs` (`execute`): end-to-end flow execution.
5. `crates/ghost-link/src/cli/gui.rs` (`launch`): GUI launch.
6. `crates/ghost-link/src/cli/gui.rs` (`check`): GUI readiness checks.
7. `crates/ghost-link/src/cli/gui.rs` (`diagnose`): GUI diagnostics report.
8. `crates/ghost-link/src/cli/join.rs` (`execute`): cluster join broadcast.
9. `crates/ghost-link/src/cli/listen.rs` (`execute`): discovery listener.
10. `crates/ghost-link/src/cli/plan.rs` (`execute`): placement plan generation.
11. `crates/ghost-link/src/cli/serve.rs` (`run`): API server startup.

## Phase 0: Architecture Decision (Day 1)

### Why this phase
The CLI stubs are in `src/cli/*`, while `src/main.rs` already contains mature implementations for the same command names.

### Work
1. Decide command architecture:
- Option A: Keep `main.rs` as command implementation source of truth and remove/retire `src/cli/*` stubs.
- Option B: Keep `src/cli/*` modules and refactor `main.rs` command handlers into shared functions consumed by CLI modules.
2. Record decision in `docs/ARCHITECTURE.md` with rationale.
3. Add CI guard that fails on new `TODO: Implement actual` strings in `crates/ghost-link/src`.

### Exit Criteria
1. A single command implementation path is selected and documented.
2. Build remains green with no behavior changes.

## Phase 1: API Stub Completion (Day 1-2)

### Scope
`crates/ghost-link/src/api/mod.rs` -> `chat_completion`.

### Work
1. Replace mock response with real inference call using the existing Ollama client used by `main.rs`.
2. Map request messages into a prompt strategy with clear role handling (`system`, `user`, `assistant`).
3. Add robust fallback and error mapping:
- `503` when backend unavailable.
- `504` for timeout.
- `500` for unexpected errors.
4. Preserve OpenAI-compatible response shape and include stable request IDs.

### Tests
1. Unit tests for prompt assembly and response mapping.
2. Integration test against a mocked Ollama endpoint (success, timeout, invalid JSON).

### Exit Criteria
1. No mock text remains in API responses.
2. Endpoint behavior is deterministic under success/failure.

## Phase 2: Command Surface Unification (Day 2-3)

### Scope
`crates/ghost-link/src/cli/*.rs` and command implementations in `crates/ghost-link/src/main.rs`.

### Work
1. If Option A selected:
- Delete `src/cli/*` modules and related stale dispatch code paths if unused.
- Keep all command behavior in `main.rs` and remove dead files.
2. If Option B selected:
- Move mature command logic from `main.rs` into `src/cli/*` shared functions.
- Update dispatch so every subcommand uses the shared implementation once.
3. Ensure command help text remains accurate and complete.

### Tests
1. CLI parsing tests for all subcommands and flags.
2. Snapshot tests for `help` output and key command outputs.

### Exit Criteria
1. No duplicate command implementation trees.
2. All command stubs removed and functionality preserved.

## Phase 3: Discovery and Flow Hardening (Day 3-4)

### Scope
`join`, `listen`, `plan`, `flow`, `dashboard`, `doctor` behaviors.

### Work
1. `join`: actual broadcast and response collection with timeout, auth, and retry controls.
2. `listen`: start responder loop with one-shot and continuous modes.
3. `plan`: execute real layer assignment using runtime profile and node resources.
4. `flow`: run assignment + transport execution path with metrics and fallback handling.
5. `dashboard`: display live node health, transport status, throughput, and latency summaries.
6. `doctor`: implement structured checks (runtime/toolchain, network reachability, model availability, config validity).

### Tests
1. Integration tests for discovery join/listen on loopback.
2. Flow regression tests with deterministic fixture cluster profiles.
3. Doctor tests for strict mode failures and JSON report output.

### Exit Criteria
1. Subcommands perform real actions, not placeholder output.
2. Commands fail fast with actionable errors.

## Phase 4: GUI Command Completeness (Day 4)

### Scope
`crates/ghost-link/src/cli/gui.rs`.

### Work
1. `launch`: invoke vendored GUI script/binary with argument passthrough and environment validation.
2. `check`: validate Python/runtime dependencies, backend endpoint reachability, and port conflicts.
3. `diagnose`: emit categorized findings with fix suggestions and strict-mode nonzero exit behavior.

### Tests
1. Unit tests for dependency and endpoint checks using mocks.
2. Integration smoke test for launch command argument passthrough.

### Exit Criteria
1. GUI commands are operational and scriptable in CI/dev.
2. Strict mode correctly drives exit status for automation.

## Phase 5: Documentation and Quality Gates (Day 5)

### Work
1. Update `README.md` and `docs/INDEX.md` command references to match final implementation.
2. Add `scripts/verify_no_stub_todos.sh` (or equivalent CI step):
- fail on `TODO: Implement actual` in source paths.
3. Add release note entry in `CHANGELOG.md` for stub completion.

### Exit Criteria
1. No remaining `TODO: Implement actual` stubs in `crates/ghost-link/src`.
2. CI blocks reintroduction of unresolved stubs.

## Suggested Execution Order (Low Risk First)
1. Phase 0 architecture decision.
2. Phase 2 command unification.
3. Phase 1 API completion.
4. Phase 3 discovery/flow hardening.
5. Phase 4 GUI completion.
6. Phase 5 docs + CI gates.

## Definition of Done
1. `grep` over `crates/ghost-link/src` returns zero matches for `TODO: Implement actual`.
2. `cargo fmt --all -- --check` passes.
3. `cargo clippy --workspace --all-targets --all-features -- -D warnings` passes.
4. `cargo test --workspace --all-features` passes.
5. Updated docs and changelog are merged with the implementation.
