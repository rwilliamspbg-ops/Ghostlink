# ghostlink-gui (Studio Scaffold)

This directory contains the Ghostlink Studio desktop application scaffold.

## Structure

- src-tauri: Rust command bridge and native shell
- frontend: Svelte UI skeleton with studio layout

## Current Status

- Initial scaffold created
- Sprint 1 command bridge wired to `ghost-link` actions (`doctor`, `probe`, `flow`, `cluster-start`)
- Frontend quick actions invoke real backend commands and render output in details panel

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
