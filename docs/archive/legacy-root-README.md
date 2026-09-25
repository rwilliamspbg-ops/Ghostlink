# ARCHIVED FILES & LEGACY DOCUMENTATION

This directory contains outdated and legacy files that are no longer used.

**⚠️ NOTICE: These files are for reference only. DO NOT USE THEM.**

---

## What's Here

### `legacy-scripts/`
One-shot fix scripts, deprecated launchers, and old entry points:
- `fix_backend_registry.py`, `fix_rocm_detection.py` — One-time Rust patchers (patches applied)
- `ghostlink_gui_tkinter.py` — Old Tkinter GUI (REPLACED by modern web GUI)
- `launch.bat`, `launch-splash.bat`, `launch-splash.sh`, `launch-fast.sh` — Deprecated launchers
- `OPEN_PR.sh` — Historical PR creation instructions

### `legacy-root-docs/`
Session completion summaries, fix reports, and status tracking that have served their purpose:
- `PHASES_*_COMPLETE.md` — Phase completion artifacts (sessions 1-5)
- `FINAL_*.md`, `FIX_SUMMARY.md`, `COMPLETION_SUMMARY.md` — Historic status reports
- `API_404_FIX_FINAL.md`, `BACKEND_API_FIX.md`, etc. — One-time fix documentation
- Various PR, test, and delivery summaries

---

## Why Archived?

1. **Legacy Scripts** — Superseded by `launch-complete.sh` / `launch-complete.bat`
2. **Legacy Docs** — Superseded by `README.md`, `CHANGELOG.md`, `docs/`
3. **One-shot Fixes** — Patches were applied; scripts/docs kept only for reference
4. **Session Artifacts** — Phase completion summaries are no longer needed day-to-day

---

## Current Setup

Use instead:
- **Launchers**: `launch-complete.sh` (Linux), `launch-complete.bat` / `launch-ollama.bat` (Windows)
- **Backend**: `cargo run -p ghost-link -- serve` — or use the launchers above
- **GUI**: `ghostlink_gui_modern/` (React/TypeScript)
- **Docs**: `README.md`, `CHANGELOG.md`, `docs/`
- **CI/Validation**: `scripts/` test harnesses

---

**Everything you need is in the parent directory. ✅**
