# Contributing to Ghost-Link

## Prerequisites

- stable Rust via `rustup`
- Git

## Setup

```bash
git clone https://github.com/rwilliamspbg-ops/Ghostlink.git
cd Ghostlink
. "$HOME/.cargo/env"
cargo build --workspace
```

## Before Opening a PR

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Pre-Push Checklist (Required)

Run these before pushing branch updates:

```bash
# 1) Rust correctness and style
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

# 2) Platform awareness
# CI runs the same checks on ubuntu-latest, windows-latest, macos-latest.
# If you changed platform-specific code (#[cfg(unix)] or #[cfg(windows)]),
# verify it compiles on the target platform.

# 3) Performance (if runtime/transport code changed)
# Check baseline benchmarks haven't regressed:
cargo bench --package ghostlink-core

# 4) Security and dependency checks
cargo audit
```

### If you changed transport, ring buffer, or pipeline code

Include benchmark results in the PR body:

```bash
cargo bench --package ghostlink-core
```

Key metrics to report:
- `in-process` throughput (tok/s) and latency (ms) at 1024 tok
- `TCP loopback` throughput (tok/s) and latency (ms) at 1024 tok
- `ring_buffer: SPSC batch cross-thread` ops/sec

### If you changed hardware detection or system profile

Run the probe command and verify no regressions:

```bash
cargo run -p ghost-link -- probe my-node --full
```

## Test Location

- Unit tests live alongside the code they test (`#[cfg(test)] mod tests`)
- Integration tests live in `crates/ghostlink-core/tests/`
- Multi-node and NPU tests live in `crates/ghostlink-core/tests/`

## Documentation Expectations

If behavior changes, update the relevant docs in:

- `README.md`
- `CHANGELOG.md`
- `docs/INDEX.md`
- `ghostlink.toml` (if config keys changed)

If a status document is no longer current, move it to `docs/archive/` and update
`docs/archive/INDEX.md`.

## PR Expectations

- keep changes focused
- include validation commands in the PR body
- call out host-specific caveats if runtime detection or probe behavior changes
- if performance metrics change, include before/after benchmark results
- if new Clippy lints trigger on CI (different Rust version), fix them before pushing

## Scope Guidance

- Prefer atomic PRs that target one theme (runtime, GUI, perf governance, or CI plumbing).
- If cross-cutting changes are unavoidable, include a short risk section and rollback strategy in the PR body.
- For large feature deliveries, consider a sequence of smaller stacked PRs.

## Release Rubric

For release-oriented PRs, include a checklist based on:

1. **Hard gates (must pass)**
   - CI: ubuntu, windows, macos — all green
   - `cargo fmt --all --check` — OK
   - `cargo clippy --workspace --all-targets -- -D warnings` — OK
   - `cargo test --workspace` — all pass
   - Performance baseline — no regression

2. **Scoring factors (for readiness trend)**
   - Documentation completeness
   - Changelog updated
   - Version bumped
   - Operational caveats documented

3. **Final recommendation**
   - GO / Conditional GO / NO-GO

## Code of Conduct

### Our Pledge

We pledge to make participation in this project a harassment-free experience for everyone, regardless of age, body size, disability, ethnicity, gender identity, level of experience, nationality, personal appearance, race, religion, or sexual identity and orientation.

### Our Standards

Examples of behavior that contributes to a positive environment:

- Using welcoming and inclusive language
- Being respectful of differing viewpoints and experiences
- Gracefully accepting constructive criticism
- Focusing on what is best for the community
- Showing empathy towards other community members

Examples of unacceptable behavior:

- Trolling, insulting/derogatory comments, and personal or political attacks
- Public or private harassment
- Publishing others' private information without explicit permission
- Other conduct which could reasonably be considered inappropriate

### Enforcement

Project maintainers are responsible for clarifying standards of acceptable behavior and will take appropriate and fair corrective action in response to any instances of unacceptable behavior.

Instances of abusive, harassing, or otherwise unacceptable behavior may be reported by opening an issue or contacting the project maintainers directly.
