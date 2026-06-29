# ghostlink-gui (Studio Scaffold)

This directory contains the Ghostlink Studio desktop application scaffold.

## Structure

- src-tauri: Rust command bridge and native shell
- frontend: Svelte UI skeleton with studio layout

## Current Status

- Initial scaffold created
- Sprint 1 command bridge wired to `ghost-link` actions (`doctor`, `probe`, `flow`, `cluster-start`)
- Frontend quick actions invoke real backend commands and render output in details panel
- Home/Cluster/Doctor tabs are interactive and backed by runtime command calls
- Startup snapshot now reports environment/config readiness cards from backend checks
- Models tab verifies Hugging Face repository/file accessibility via `scripts/verify_hf_models.py`
- Chat tab provides prompt/model/parameter controls with backend-selection preview responses
- Cluster tab now renders live node cards with health indicators from parsed `probe` output
- Models tab includes preset catalog shortcuts for common smoke/target repos
- Chat tab keeps a recent exchange history for iterative testing
- First-launch onboarding modal guides users through Cluster/Models/Chat/Doctor flow
- Settings tab now includes theme, font scaling, reduced-motion, and high-contrast preferences persisted locally
- Home tab now supports one-click `fast` and `full` validation tiers with structured step results
- Settings now supports profile export/import bundles for portable Studio + config defaults
- Home tab now tracks recent snapshot and validation run history trends

## Dev Notes

This scaffold is intentionally isolated from the Rust workspace build until Sprint 1 integration is complete.

## Local Run (Scaffold)

1. `cd crates/ghostlink-gui/frontend && npm install`
2. `cd ../src-tauri && cargo tauri dev`

The app currently shells out to:

- `cargo run -p ghost-link -- doctor`
- `cargo run -p ghost-link -- probe studio-local fast`
- `cargo run -p ghost-link -- flow ...`
- `cargo run -p ghost-link -- cluster-start ...`

And reads startup snapshot checks via:

- `studio_snapshot` Tauri command (toolchain/python/config/doctor artifact state)
