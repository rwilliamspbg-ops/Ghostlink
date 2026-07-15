# Testing

This file acts as the top-level testing entrypoint for the repository's documentation and CI consistency checks.

For the detailed test matrix, validated commands, and current gaps, see [docs/archive/TESTING.md](docs/archive/TESTING.md).

## Core validation commands

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
python3 scripts/verify_hf_models.py
```

## Notes

- The project uses a Cargo workspace layout.
- CI and local validation should be treated as the source of truth for current test coverage and runtime checks.
