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
