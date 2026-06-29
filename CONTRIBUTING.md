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
cargo fmt
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

If you touch integration behavior, also run:

```bash
cargo test -p ghostlink-core --test integration
```

If you touch model/bootstrap/download workflow, also run:

```bash
python3 scripts/verify_hf_models.py
```

## Test Location

Package-owned integration tests live in `crates/ghostlink-core/tests/`.

## Documentation Expectations

If behavior changes, update the relevant docs in:

- `README.md`
- `TESTING.md`
- `docs/ARCHITECTURE.md`
- `docs/EXAMPLES.md`

## PR Expectations

- keep changes focused
- include validation commands in the PR body
- call out host-specific caveats if runtime detection or probe behavior changes

## Scope Guidance

- Prefer atomic PRs that target one theme (runtime, GUI, perf governance, or CI plumbing).
- If cross-cutting changes are unavoidable, include a short risk section and rollback strategy in the PR body.
- For large feature deliveries, consider a sequence of smaller stacked PRs.

## Release Rubric

For release-oriented PRs, include a checklist based on:

- required CI gates (production gate, tests, lint)
- runtime/perf checks (baseline drift, stage-tail/canary where applicable)
- GUI readiness checks (if GUI code changed)
- documentation completeness and operational caveats

Recommended rubric reporting format:

1. Hard gates (must pass)
2. Weighted score (for readiness trend tracking)
3. Final recommendation (GO / Conditional GO / NO-GO)
