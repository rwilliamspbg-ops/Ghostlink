# Ghost-Link Verification

## Current Status

The workspace currently validates successfully with:

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
python3 scripts/verify_hf_models.py
```

GitHub Actions now also exposes split workflow status for docs, lint, tests, and nightly Hugging Face model verification.

## Current Structure

```text
crates/ghostlink-core/src/
├── accelerator.rs
├── cluster.rs
├── dashboard.rs
├── health.rs
├── host.rs
├── load_balance.rs
├── planning.rs
├── protocol.rs
├── ring.rs
└── xdp.rs

crates/ghostlink-core/tests/
├── common.rs
└── integration.rs
```

## Verified Areas

- runtime detection with fast and full probe modes
- runtime-selected execution backend selection
- planning and load balancing autotuning
- package-owned integration suite
- bench targets compile under clippy

## Current Test Totals

- 3 CLI tests
- 69 library tests
- 7 shared integration-target tests
- 27 integration tests
- 106 total passing tests
