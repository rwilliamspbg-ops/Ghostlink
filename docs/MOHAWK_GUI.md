# Mohawk GUI Integration

Ghostlink includes a vendored copy of the Mohawk GUI sources in [third_party/mohawk_gui](../third_party/mohawk_gui).

## Run

1. Install Python dependencies:

```bash
python3 -m pip install -r third_party/mohawk_gui/requirements.txt
```

If you are in a Linux container, also install system OpenGL runtime packages:

```bash
# Debian/Ubuntu
sudo apt-get update
sudo apt-get install -y libgl1 libxkbcommon0
```

2. Launch via Ghostlink CLI:

```bash
cargo run -p ghost-link -- gui
```

Validate readiness (LM Studio-style preflight report):

```bash
cargo run -p ghost-link -- gui-check
```

Fail CI/automation if anything is missing:

```bash
cargo run -p ghost-link -- gui-check --strict
```

3. Pass Mohawk GUI arguments through Ghostlink:

```bash
cargo run -p ghost-link -- gui --host 0.0.0.0 --port 8003
```

## Environment

- `GHOSTLINK_PYTHON`: overrides the Python executable used by `ghost-link gui` (default: `python3`).

## Notes

- This integration vendors Mohawk GUI source files and launches the GUI process from the Rust CLI.
- GUI behavior and dependencies come from the upstream Mohawk project.
- In headless/devcontainer environments, GUI startup can fail with `libGL.so.1` or `libxkbcommon.so.0` missing unless system packages are installed.
- `ghost-link gui` runs Linux preflight checks for `libGL.so.1` and `libxkbcommon.so.0` and exits early with install instructions if missing.
- `ghost-link gui-check` reports missing Python modules, system OpenGL dependencies, and headless display environment status.
