# ghostlink-gui (Ghostlink Studio) — superseded, not actively maintained

> **This Tauri + Svelte desktop shell is not the GUI Ghostlink ships today.**
> It predates and is not wired into the current launchers (`launch.sh` /
> `launch.bat`), the root Cargo workspace, or the release pipeline. The
> active, maintained GUI is [`ghostlink_gui_modern/`](../../ghostlink_gui_modern)
> (React + Vite), which every launcher builds and every recent CHANGELOG
> entry documents. This crate is kept for reference and possible future
> revival, but treat it as archived: don't expect it to build against the
> current backend without changes, and don't add new features here without
> first checking with a maintainer.

This directory contains an earlier prototype of a Ghostlink Studio desktop application.

## Structure

- src-tauri: Rust command bridge and native shell
- frontend: Svelte UI skeleton with studio layout

## Current Status

- Initial app shell created
- Sprint 1 command bridge wired to `ghost-link` actions (`doctor`, `probe`, `flow`, `cluster-start`)
- Frontend quick actions invoke real backend commands and render output in details panel
- Home/Cluster/Doctor tabs are interactive and backed by runtime command calls
- Startup snapshot now reports environment/config readiness cards from backend checks
- Models tab verifies Hugging Face repository/file accessibility via `scripts/verify_hf_models.py`
- Chat tab provides prompt/model/parameter controls with live flow-backed runtime responses
- Studio launcher now defaults chat routing to distributed Rust backend execution for multi-node layer split testing
- Cluster tab now renders live node cards with health indicators from parsed `probe` output
- Models tab includes preset catalog shortcuts for common smoke/target repos
- Chat tab keeps a recent exchange history for iterative testing
- First-launch onboarding modal guides users through Cluster/Models/Chat/Doctor flow
- Settings tab now includes theme, font scaling, reduced-motion, and high-contrast preferences persisted locally
- Home tab now supports one-click `fast` and `full` validation tiers with structured step results
- Settings now supports profile export/import bundles for portable Studio + config defaults
- Home tab now tracks recent snapshot and validation run history trends
- Cluster tab now supports worker discovery, multi-select batch connect, and per-worker quick TCP checks
- Cluster/connection preferences now persist in both local UI prefs and profile export/import bundles

## Dev Notes

The Studio app is intentionally kept as a focused Tauri + Svelte surface while invoking the workspace CLI/runtime commands.

## Local Run

1. `cd crates/ghostlink-gui/frontend && npm install`
2. `cd ../src-tauri && cargo tauri dev`

## Easy Launch (Recommended)

From repository root:

1. Preflight check only: `bash scripts/launch_studio.sh --check`
2. Launch Studio shell: `bash scripts/launch_studio.sh`

Performance defaults applied by the launcher (override with environment variables when needed):

- `GHOSTLINK_FLOW_DEFAULT_TRANSPORT=tcp`
- `GHOSTLINK_TCP_MAX_INFLIGHT=256`
- `GHOSTLINK_TCP_AUTOTUNE=1`
- `GHOSTLINK_FLOW_ENABLE_REBALANCE=1`
- `GHOSTLINK_CHAT_EXEC_TOKENS=256`
- `GHOSTLINK_CHAT_MICRO_BATCH=8`

Chat proxy mode can be selected with:

- `GHOSTLINK_STUDIO_CHAT_BACKEND=backend` (default, recommended for distributed flow)
- `GHOSTLINK_STUDIO_CHAT_BACKEND=ollama` (compatibility mode for direct Ollama chat)

For real backend wiring verification:

1. Preflight check only: `bash scripts/run_gui_real_stack.sh 127.0.0.1 8003 --check`
2. Launch GUI against real backend: `bash scripts/run_gui_real_stack.sh 127.0.0.1 8003`

## Cluster Usage (Step-by-Step)

1. Open the Cluster tab.
2. In Discovery Settings, keep default node hints or add IDs (comma-separated), then click Discover Workers.
3. Use Set Local on the machine that will initiate flow.
4. Use Set Remote for a target worker, or check Include in batch connect on multiple reachable workers.
5. Optional: run Quick TCP Test per worker with host/port and timeout to quickly validate network reachability.
6. Run Connect Local -> Remote for one target, or Connect Selected/Reachable for multi-worker batch execution.
7. Review Batch Connect Results for per-worker pass/fail and runtime.

## Profile Portability

Exported profiles now include:

- UI accessibility/theme preferences
- Model + chat defaults
- Cluster discovery hints and probe mode
- Local/remote node mapping and flow transport settings
- Flow token/micro-batch values
- Cluster start defaults and advanced button visibility

Use Settings -> Profile Portability to export/import profiles across machines.

## Packaging

Create release bundle from repository root:

- Signed bundle: `bash scripts/release_bundle.sh artifacts/release/v1.0.0 signed`
- Unsigned bundle: `bash scripts/release_bundle.sh artifacts/release/v1.0.0 unsigned`

The app currently shells out to:

- `cargo run -p ghost-link -- doctor`
- `cargo run -p ghost-link -- probe studio-local fast`
- `cargo run -p ghost-link -- flow ...`
- `cargo run -p ghost-link -- cluster-start ...`

And reads startup snapshot checks via:

- `studio_snapshot` Tauri command (toolchain/python/config/doctor artifact state)
