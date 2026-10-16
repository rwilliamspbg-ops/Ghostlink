# Ghostlink task runner
# Usage: just <command>

default: verify

# Run all verification gates
verify: fmt clippy test

# Format check
fmt:
    cargo fmt --all --check

# Lint
clippy:
    cargo clippy --workspace --all-targets -- -D warnings

# Test all crates
test:
    cargo test --workspace

# Test a specific crate
test-crate crate:
    cargo test -p {{crate}}

# Build all
build:
    cargo build --workspace

# Build release
build-release:
    cargo build --release --workspace

# Run probe
probe:
    cargo run -p ghost-link -- probe my-node --full

# Bench
bench:
    cargo bench --package ghostlink-core

# Clean
clean:
    cargo clean

# Format (apply)
fix:
    cargo fmt --all
