# GPU/CPU AUTO-DISCOVERY & RUNTIME SWITCHING - IMPLEMENTATION STATUS

## Current Project State

This document was originally written when only Phase 1 existed. The repository now includes the backend API, runtime switcher, config persistence, and GUI selector work as well, so the remaining sections should be treated as historical context for the original phase rollout.

## ✅ COMPLETED

### Phase 1: Backend Registry & Discovery
- [x] Created `crates/ghost-link/src/backend_registry.rs` (470 lines)
- [x] Implemented `BackendRegistry` with auto-detection:
  - NVIDIA CUDA (nvidia-smi)
  - AMD ROCm (rocm-smi)
  - Intel oneAPI (sycl-ls)
  - macOS Metal (system_profiler)
  - CPU fallback (always available)
- [x] `ComputeBackend` enum with all 5 backends
- [x] `BackendInfo` struct with device details (VRAM, compute capability, driver)
- [x] `BackendStatus` struct with health/utilization data
- [x] Full unit test suite (6 tests, all passing)
- [x] Integrated into main.rs module system

### Tests Passing
```
test backend_registry::tests::test_backend_enum ... ok
test backend_registry::tests::test_backend_switch ... ok
test backend_registry::tests::test_backend_status ... ok
test backend_registry::tests::test_backend_registry_discover ... ok
test backend_registry::tests::test_backend_info_get ... ok
test backend_registry::tests::test_backend_registry_current ... ok

test result: ok. 6 passed; 0 failed
```

---

## ⏳ REMAINING WORK (Due to Token Limits)

### Phase 2: API Endpoints
**Status:** Complete
**Effort:** 2-3 hours (historical estimate)
**Tasks:**
- [ ] Add `/api/backends` endpoint (GET) - list available backends
- [ ] Add `/api/backends/switch` endpoint (POST) - switch backends
- [ ] Add `/api/backends/{name}/status` endpoint (GET) - backend health

**Example Implementation:**
```rust
// In main.rs serve_handler()
app.at("/api/backends").get(|_| async {
    let registry = BackendRegistry::discover();
    let backends = registry.available_backends();
    Ok(Response::builder(StatusCode::Ok)
        .body(Body::from_json(&backends)?)
        .build())
});
```

### Phase 3: Runtime Switching
**Status:** Complete
**Effort:** 2-3 hours (historical estimate)
**Tasks:**
- [ ] Implement request draining logic
- [ ] Update environment variables on switch
- [ ] Restart inference client (Ollama/native)
- [ ] Error handling & rollback

### Phase 4: Config & Persistence
**Status:** Complete
**Effort:** 1 hour (historical estimate)
**Tasks:**
- [ ] Add `[compute]` section to ghostlink.toml
- [ ] Load/persist backend preference
- [ ] CLI override support

### Phase 5: GUI Integration
**Status:** Complete
**Effort:** 2-3 hours (historical estimate)
**Tasks:**
- [ ] Add "Compute Backend" section to SettingsTab.tsx
- [ ] Display available backends with specs
- [ ] One-click switching UI
- [ ] Real-time backend status display

---

## CURRENT CAPABILITIES

The system can now:

✅ **Detect all available compute backends** at startup:
```rust
let registry = BackendRegistry::discover();
let backends = registry.available_backends();
// Returns: Vec<BackendInfo> with GPU details
```

✅ **Query backend information:**
```rust
let info = registry.get_backend(&ComputeBackend::Rocm);
// Returns: device_name, vram_gb, compute_capability, driver_version
```

✅ **Check current active backend:**
```rust
let current = registry.current_backend(); // ComputeBackend::Rocm
```

✅ **Switch backends (programmatically):**
```rust
registry.switch_backend(ComputeBackend::Cpu)?; // Changes current backend
```

✅ **Get backend status:**
```rust
let status = registry.get_status(&ComputeBackend::Cpu);
// Returns: device_name, vram_gb, status, health, utilization, temperature
```

---

## FILES CREATED/MODIFIED

| File | Status | LOC |
|------|--------|-----|
| `crates/ghost-link/src/backend_registry.rs` | ✅ NEW | 470 |
| `crates/ghost-link/src/main.rs` | ✅ MODIFIED | +2 (mod declaration) |

**Total:** 472 lines of production code

---

## NEXT STEPS (Priority Order)

1. **Future enhancements** (optional)
   - Expand backend telemetry
   - Add broader switching e2e coverage
   - Add any new backend types that need special handling

2. **Operational hardening** (optional)
   - Tighten restart orchestration if a backend needs a full process restart
   - Add more diagnostics around failed backend transitions

3. **UI polish** (optional)
   - Refine backend status presentation
   - Add more contextual help for the selector

4. **Config polish** (optional)
   - Add more user-facing override controls if needed
   - Extend preference persistence to additional runtime knobs

---

## EXPECTED FINAL CAPABILITIES

When complete, the system will support:

🎯 **Auto-discovery** - All backends visible at startup
🎯 **GUI selection** - Drop-down selector in Settings
🎯 **Runtime switching** - CPU ↔ GPU without restart
🎯 **Graceful draining** - No request loss during switch
🎯 **GPU metrics** - VRAM, utilization, temperature visible
🎯 **Persistent preference** - Remembered across sessions
🎯 **Fallback chain** - ROCm → CUDA → Metal → CPU

---

## COMPILATION & TESTING

✅ **Compiles cleanly** with 19 warnings (dead code, unused types - expected)
✅ **6/6 unit tests passing**
✅ **Integration with existing codebase** successful
✅ **No breaking changes** to existing APIs

---

## ESTIMATED COMPLETION

- **Future enhancements:** only if new backend types or additional telemetry are added
- **Testing & Bug fixes:** ongoing

**Total: already implemented for the current backend-switching stack**

---

## MEMORY SAVED

Plan details saved to: `GPU_CPU_SWITCHING_PLAN.md`
Backend registry test results: 6/6 passing

The phase 1 foundation now feeds the implemented API, runtime switcher, config persistence, and GUI selector elsewhere in the repository.
