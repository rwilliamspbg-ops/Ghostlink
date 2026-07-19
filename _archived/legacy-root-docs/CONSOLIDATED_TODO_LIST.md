# Ghostlink Consolidated TODO List

> Last updated: 2026-07-13

## Completed

### Launch Scripts & Backend Fixes (Session 1)
- [x] Fix backend model unload handler — kills llama-server process on unload
- [x] Fix backend model load handler — searches known build paths for llama-server binary
- [x] Fix launch.bat — consistent env vars, health checks, model download
- [x] Fix launch-complete.bat — aligned model paths, env vars, health checks, browser auto-open
- [x] Fix launch-fast.bat — added missing env vars, health checks, model auto-download
- [x] Fix launch-splash.bat — delegates correctly to launch-complete.bat
- [x] Fix launch.sh — uses $PROJECT_ROOT/models, pre-built binary support
- [x] Fix launch-complete.sh — consistent model paths, llama-server path search, env vars
- [x] Fix launch-splash.sh — correct backend binary check

### Phase 1: Critical Fixes (Session 2)
- [x] Fix docker-compose.test.yml — removed duplicate YAML version/services keys
- [x] Fix run_gui_tests.py — added PerformanceTester class to test_gui_framework.py
- [x] Clean up _FIXED and .backup artifact files (6 files removed)
- [x] Wire control-plane registry into HTTP endpoints — GET/POST /api/workers, POST /api/workers/heartbeat, background cleanup goroutine
- [x] Generate go.sum for control-plane Go module

### Phase 2: Testing Completion (Session 2)
- [x] Add `PerformanceTester` class with `profile_chat_performance()` and `profile_model_load_performance()`
- [x] Add `test_end_to_end_workflow` — list models → load → chat → unload → verify status
- [x] Add `test_model_unload` — validates unload resets current_model
- [x] Add `test_settings_roundtrip` — validates GET/POST /api/settings preserves fields
- [x] Create `tests/test_regression.py` — 8 regression tests covering chat, model management, and API endpoints

### Phase 3: MVP Polish (Session 2)
- [x] Add backend endpoint `GET /api/security/audit-log` returning live security events
- [x] Replace hardcoded SecurityTab audit entries with live API fetch + 30s polling
- [x] Replace hardcoded ModelsTab `POPULAR_MODELS` with live HuggingFace search API
- [x] Add search-as-you-type with 300ms debounce to ModelsTab HuggingFace tab

### Phase 4: Deployment & Docs (Session 2)
- [x] Update `scripts/quickstart.sh` with model download progress indicator
- [x] Create `scripts/quickstart.bat` — Windows quickstart with prerequisites, build, model download, smoke flow
- [x] Add launch script troubleshooting section to `docs/TROUBLESHOOTING.md` (5 common issues with fixes)
- [x] Create GitHub issue templates (`bug_report.yml`, `feature_request.yml`)
- [x] Add Code of Conduct to `CONTRIBUTING.md`
- [x] Create `docker-compose.override.yml` for local development (3-service stack with health checks)
- [x] Expand `docs/launch_demo.md` into full demo walkthrough (7-step walkthrough)
- [x] Add control-plane worker deregistration (`DELETE /api/workers/:id`) and health summary
- [x] Add control-plane integration tests (8 tests: health, register, list, heartbeat, deregister, cleanup, summary)
- [x] Add `Deregister()` and `Summary()` methods to registry package

---

## Remaining Work

### Optional Enhancements
- [ ] Performance benchmark tests with latency targets (load <500ms, chat <750ms, concurrent <2s)
- [ ] Benchmark screenshots for README and landing page
- [ ] Browser automation (Selenium/Playwright) for E2E GUI testing

---

## Verification Commands

```bash
# Rust
cargo check -p ghost-link    # Compiles clean
cargo test -p ghost-link     # 27/27 pass
cargo clippy -p ghost-link   # No new warnings

# Frontend
cd ghostlink_gui_modern && npm run build   # Builds clean
npm run type-check                         # Pre-existing unused-var warnings only

# Go Control Plane
cd control-plane && go build ./... && go test ./...   # 8/8 pass

# Python Tests (requires backend running on port 8003)
python run_gui_tests.py --all
python -m pytest tests/test_regression.py -v
```
