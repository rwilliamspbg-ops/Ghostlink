# GPU/CPU Auto-Discovery & Runtime Switching - Concrete Plan

## Current State Analysis

### ✅ What Already Exists:
1. **InferenceBackend enum** (main.rs:118)
   - Ollama backend (wrapper around Ollama API)
   - Native backend (direct llama.cpp integration)
   - Env var control: `GHOSTLINK_INFERENCE_BACKEND=ollama|native`
   - Static selection at startup only

2. **Host Detection Module** (ghostlink-core/src/host.rs)
   - `RuntimeProfile` struct with GPU backend info
   - `detect_runtime_profile()` - detects CPU cores, GPU type
   - `GpuBackend` enum: Cuda, Rocm, OneAPI, Metal, Cpu
   - `AccelerationMode` enum: Gpu, Neon (NPU), Cpu

3. **Ollama Client** (ollama.rs)
   - Full API bindings
   - Model list, chat, generate, streaming support
   - NO backend switching capability

4. **Native Engine** (native_engine.rs)
   - Direct llama.cpp integration via llama-server subprocess
   - GPU layers configurable via NGL environment var
   - NO backend switching capability

5. **Hardware Detection** (launch.sh / launch-ollama.bat)
   - GPU detection: nvidia-smi, rocm-smi, lspci, system_profiler
   - Already auto-detecting AMD ROCm with gfx906 mapping

### ❌ What's Missing:
1. **Runtime backend switching** - currently static
2. **GPU memory/VRAM tracking** - not exposed in API
3. **Backend health/status endpoint** - not implemented
4. **Backend availability query endpoint** - not implemented
5. **GPU/compute preference persistence** - not stored
6. **GUI backend selector** - not in SettingsTab

---

## Implementation Plan

### Phase 1: Backend Registry & Discovery (Immediate)

**Files to Create:**
- `crates/ghost-link/src/backend_registry.rs` (new)

**Features:**
```rust
pub struct BackendInfo {
    pub name: String,              // "cuda", "rocm", "metal", "cpu"
    pub available: bool,
    pub device_name: String,       // "NVIDIA RTX 4090", "AMD Radeon 860M"
    pub vram_gb: Option<f32>,
    pub compute_capability: String, // "sm_90", "gfx906"
    pub driver_version: String,
}

pub struct BackendRegistry {
    backends: Vec<BackendInfo>,
    current: String,
}

impl BackendRegistry {
    pub fn discover() -> Self { /* query nvidia-smi, rocm-smi, etc */ }
    pub fn available_backends(&self) -> Vec<&BackendInfo>
    pub fn get_backend(&self, name: &str) -> Option<&BackendInfo>
}
```

**Detection Logic:**
- Check `nvidia-smi` → CUDA backend
- Check `rocm-smi` → ROCm backend  
- Check Intel oneAPI (iGPU) → oneAPI backend
- Check macOS Metal availability → Metal backend
- Always available: CPU fallback

---

### Phase 2: API Endpoints (Core Backend)

**Files to Modify:**
- `crates/ghost-link/src/main.rs` - add routes

**New HTTP Endpoints:**
```
GET /api/backends
  Returns: {
    "available": [
      {"name": "rocm", "device": "AMD Radeon 860M", "vram_gb": 14.2, "status": "active"},
      {"name": "cpu", "device": "AMD Ryzen AI 7 350", "vram_gb": null, "status": "ready"}
    ],
    "current": "rocm"
  }

POST /api/backends/switch
  Payload: {"backend": "cpu"}
  Returns: {"status": "switched", "backend": "cpu", "restart_required": false}

GET /api/backends/{name}/status
  Returns: {"name": "rocm", "health": "healthy", "utilization": 24.5, "temperature": 45.0}
```

---

### Phase 3: Runtime Switching Implementation

**Files to Modify:**
- `crates/ghost-link/src/main.rs` - add switch_backend() function
- `crates/ghost-link/src/ollama.rs` - make client swappable
- `crates/ghost-link/src/native_engine.rs` - support dynamic config

**Logic:**
1. On `/api/backends/switch` POST:
   - Validate backend available in registry
   - Queue any in-flight requests to drain (timeout 30s)
   - Update env vars (HIP_PLATFORM, HSA_OVERRIDE_GFX_VERSION, etc)
   - Restart inference client (Ollama or native)
   - Return status

**Environment Variables to Control:**
```
For ROCm:
  HSA_OVERRIDE_GFX_VERSION=gfx906  (or detect from system)
  HIP_PLATFORM=amd
  OLLAMA_NUM_THREAD=16

For CUDA:
  CUDA_VISIBLE_DEVICES=0
  TF_CPP_MIN_LOG_LEVEL=2

For CPU:
  OLLAMA_NUM_THREAD=<all_cores>
  HSA_OVERRIDE_GFX_VERSION=none
```

---

### Phase 4: Configuration & Persistence

**Files to Create/Modify:**
- Update `ghostlink.toml` with new section

**Config Section:**
```toml
[compute]
preferred_backend = "rocm"          # fallback order: rocm → cuda → cpu
auto_discover = true
gpu_memory_allocation = 0.80        # 80% safe threshold
```

**Store Preference:**
- Load from config file on startup
- Update via API and persist to disk

---

### Phase 5: GUI Integration

**Files to Modify:**
- `ghostlink_gui_modern/src/components/SettingsTab.tsx` (new section)
- `ghostlink_gui_modern/src/api.ts` - add backend API calls
- `ghostlink_gui_modern/src/store.ts` - add backend state

**UI Components:**
```
[Settings Tab]

Compute Backend
┌─────────────────────────────────┐
│ Current: AMD ROCm (14.2 GB)     │
│                                 │
│ Available backends:             │
│ ☑ ROCm (14.2 GB) [Active]      │
│   • Device: AMD Radeon 860M     │
│   • Driver: ROCm 6.1            │
│                                 │
│ ☐ CPU (Intel Core i9)          │
│   • Cores: 16                   │
│   • RAM: 28 GB                  │
│                                 │
│ [Switch to CPU]                 │
└─────────────────────────────────┘

GPU Memory Allocation: [========▮    ] 80%
```

---

## Concrete Task Breakdown

### Week 1: Foundation
- [ ] Task 1a: Create `backend_registry.rs` with discovery
- [ ] Task 1b: Add backend detection (rocm-smi, nvidia-smi)
- [ ] Task 1c: Unit tests for discovery

### Week 1-2: API
- [ ] Task 2a: Add `/api/backends` endpoint
- [ ] Task 2b: Add `/api/backends/switch` endpoint
- [ ] Task 2c: Add health check per backend
- [ ] Task 2d: Integration tests for switching

### Week 2: Runtime
- [ ] Task 3a: Implement env var updates
- [ ] Task 3b: Handle request draining on switch
- [ ] Task 3c: Restart inference client
- [ ] Task 3d: Error handling & rollback

### Week 2-3: Config
- [ ] Task 4a: Add config section to ghostlink.toml
- [ ] Task 4b: Load/persist backend preference
- [ ] Task 4c: CLI override support

### Week 3: UI
- [ ] Task 5a: Add SettingsTab backend selector
- [ ] Task 5b: Call API endpoints from GUI
- [ ] Task 5c: Display backend info & status
- [ ] Task 5d: E2E testing

---

## Impact & Benefits

✅ Users can see all available compute backends at startup
✅ One-click backend switching in GUI (CPU ↔ GPU)
✅ Graceful request handling during switch
✅ GPU memory/capability visibility
✅ Persistent backend preference
✅ Fallback chain (ROCm → CUDA → CPU)

---

## Priority

**High Priority (enables everything else):**
1. Backend registry discovery (Task 1)
2. `/api/backends` endpoint (Task 2a)
3. Runtime switching logic (Task 3)

**Medium Priority (user-facing):**
4. `/api/backends/switch` endpoint (Task 2b)
5. GUI backend selector (Task 5)

**Low Priority (nice-to-have):**
6. Health monitoring per backend (Task 2c)
7. Config persistence (Task 4)

---

## Files to Create/Modify Summary

### Create:
- `crates/ghost-link/src/backend_registry.rs` (200 lines)

### Modify:
- `crates/ghost-link/src/main.rs` (add ~150 lines for routes & logic)
- `ghostlink_gui_modern/src/components/SettingsTab.tsx` (add ~80 lines)
- `ghostlink_gui_modern/src/api.ts` (add ~20 lines)
- `ghostlink_gui_modern/src/store.ts` (add ~15 lines)
- `ghostlink.toml` (add 5-10 lines)

### Total LOC: ~470 lines new code, minimal changes to existing

---

**Recommendation: Start with Phase 1 & 2a this week. GPU switching will be available immediately after.**
