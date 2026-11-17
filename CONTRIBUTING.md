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
# Quick verification (recommended)
just verify

# Or run individually:
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
- `docs/archive/INDEX.md` (if archiving a status document)
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

## Tag & Release Publication Flow

1. **Version Consistency**: Release PRs must bump product version across all publishable manifests to `X.Y.Z` (`crates/ghostlink-core/Cargo.toml`, `crates/ghost-link/Cargo.toml`, `ghostlink_gui_modern/package.json`, `sdks/js/package.json`, `sdks/python/pyproject.toml`) and `CHANGELOG.md` (`## [X.Y.Z] - YYYY-MM-DD`).
2. **Automated Tagging**: Upon merging a release PR to `main`, `.github/workflows/tag-release-on-main.yml` validates version consistency across all manifests and creates/pushes tag `vX.Y.Z`.
3. **Publish Workflows** (manual dispatch): the tag is created by the automation
   as `github-actions[bot]` with the workflow's `GITHUB_TOKEN`, and GitHub does
   **not** start further workflows from `GITHUB_TOKEN` pushes — so the tag alone
   does not publish anything. Dispatch each workflow against the tag:

   ```bash
   gh workflow run publish-crates.yml    --repo <owner>/<repo> --ref vX.Y.Z
   gh workflow run publish-sdks.yml      --repo <owner>/<repo> --ref vX.Y.Z
   gh workflow run release-artifacts.yml --repo <owner>/<repo> --ref vX.Y.Z
   ```

   - `.github/workflows/publish-crates.yml` (crates.io publication for `ghostlink-core` and `ghost-link`)
   - `.github/workflows/publish-sdks.yml` (npm and PyPI publication for JavaScript and Python SDKs)
   - `.github/workflows/release-artifacts.yml` (GitHub Release asset bundles, SHA256SUMS, SBOM, provenance)

   Each workflow opens with a "Require a tag ref" guard that fails loudly if it
   is dispatched against a branch instead of the tag.
4. **Manual Tag Fallback**: If workflow permissions prevent automatic tagging on
   main, create and push the tag manually — on **`main`'s merge commit**, never
   the release-branch tip:
   ```bash
   git fetch origin
   git checkout main && git pull --ff-only origin main
   git tag -a vX.Y.Z -m "Ghostlink vX.Y.Z" && git push origin vX.Y.Z
   ```
   Then dispatch the three publish workflows as in step 3.

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

## Plugin marketplace-lite

Custom inference backends can be registered via `plugins.toml` (mirroring
`mcp_servers.toml`). Each `[[plugin]]` entry registers an OpenAI-compatible
backend (`OpenAiCompatPlugin`) by name/base_url (api_key optional). Copy
`plugins.toml.example` to `plugins.toml` and edit. See
`crates/ghost-link/src/backend_plugin.rs`.

## Code of Conduct

This project follows the Contributor Covenant. See [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) for the full pledge, standards, and enforcement process.
