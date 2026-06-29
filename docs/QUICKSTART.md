# Quickstart

This guide is the fastest path to a successful first run.

## Goal

Complete a local Ghostlink smoke run in under 5 minutes with one command.

## 1) Clone and run

```bash
git clone https://github.com/rwilliamspbg-ops/Ghostlink.git
cd Ghostlink
bash scripts/quickstart.sh
```

Expected success output includes:

- `Smoke flow completed`
- `Quickstart completed.`

## 2) What quickstart does

1. Checks required tools (`cargo`, `python3`).
2. Creates `./ghostlink.toml` from `./ghostlink.example.toml` if missing.
3. Builds `ghost-link`.
4. Runs `cargo run -p ghost-link -- --config ./ghostlink.toml flow`.
5. Prints next-step commands.

## 3) Most common first-run fixes

### Missing cargo

```bash
curl https://sh.rustup.rs -sSf | sh -s -- -y
. "$HOME/.cargo/env"
bash scripts/quickstart.sh
```

### Missing Python

Install Python 3.10+ using your OS package manager, then rerun:

```bash
bash scripts/quickstart.sh
```

### Smoke flow fails

Use strict diagnostics and read troubleshooting:

```bash
cargo run -p ghost-link -- gui-check --strict
cargo run -p ghost-link -- --config ./ghostlink.toml flow
```

See `docs/TROUBLESHOOTING.md` for additional fix paths.

## 4) Next commands after quickstart

```bash
# Start a local 3-node discovery validation cluster
cargo run -p ghost-link -- cluster-start 3 46000

# Run full local validation suite
bash scripts/run_full_validation.sh

# Read staged deployment guidance
cat docs/DEPLOYMENT.md
```
