# Ghostlink: Device Auto-Discovery & Auto-Tuning Improvement Plan

## Overview

Ghostlink's current discovery/tuning system works but has three structural problems:

1. **Detection logic is duplicated** across `host.rs`, `runtime.rs`, and `backend_registry.rs` with slight inconsistencies
2. **Platform coverage is uneven** — macOS and Windows have significantly weaker detection than Linux
3. **Auto-tuning is shallow** — only acceleration mode is considered, not network topology, NUMA, thermal state, or heterogeneous GPU configs

The goal is **out-of-the-box usability on all three platforms** with zero config required.

---

## Phase 1: Unified `SystemProfile` (Foundation)

**Problem:** Today, `RuntimeProfile` (host.rs) lives in `ghostlink-core`, while `RuntimeDetector` (runtime.rs) and `BackendRegistry` (backend_registry.rs) live in `ghost-link`. They detect overlapping things via different code paths, and there's no single authoritative "what is this machine?" struct.

**Solution:** Introduce a single, canonical `SystemProfile` struct (in `ghostlink-core`) that collects:

```rust
pub struct SystemProfile {
    // CPU
    pub cpu: CpuInfo,            // brand, cores, arch, features (AVX2/AVX-512/NEON/SVE), NUMA topology
    // Memory
    pub memory: MemoryInfo,      // total, available, NUMA-distributed
    // GPUs (multiple)
    pub gpus: Vec<GpuInfo>,      // name, vram, backend, driver, compute cap, pci topology
    // Accelerators
    pub npus: Vec<NpuInfo>,      // NPU type, memory, driver
    // Network
    pub network: NetworkInfo,    // interfaces, MTU, latency probes
    // Derived
    pub acceleration_mode: AccelerationMode,
    pub recommended_workers: usize,
    pub system_id: String,
}
```

**Key changes:**
- `detect_runtime_profile()` → `SystemProfile::detect()` (single entry point)
- `RuntimeDetector::detect()` → folded into `SystemProfile::detect()`
- `BackendRegistry::discover()` → built from `SystemProfile`
- All three old files become thin wrappers calling `SystemProfile`
- Cross-platform detection for every field (no `#[cfg]` only paths)

---

## Phase 2: Fill Platform Gaps

### 2.1 Memory Detection

| Platform | Current | Target |
|----------|---------|--------|
| Linux | `/proc/meminfo` | Keep; add cgroups-aware fallback |
| Windows | **Missing** (env var only) | `GetPerformanceInfo` / `GlobalMemoryStatusEx` via `windows-sys` |
| macOS | **Missing** (env var only) | `sysctl hw.memsize` (partially exists in runtime.rs but not in host.rs) |

**Move both to host.rs** with a single `detect_system_memory_gb()` that uses `#[cfg]` internally.

### 2.2 GPU Detection

| Platform | Current | Target |
|----------|---------|--------|
| Linux | `nvidia-smi`, `rocm-smi`, sysfs, `lspci` | ✓ Strong; add Vulkan ICD probe |
| Windows | WMI `Win32_VideoController` | Add DirectX DXGI (`CreateDXGIFactory` → `EnumAdapters`) for more accurate VRAM |
| macOS | `system_profiler` (backend_registry only) | Add `Metal` framework probe through `metal-rs` or `objc2` bindings |

**Critical fix:** The `infer_compute_capability_from_name()` function at `host.rs:669` hardcodes NVIDIA/AMD/Intel name matching. For non-`rocm` builds, AMD GPUs are labeled "gpu" rather than "rocm". This should use a feature-name-to-backend map that works regardless of compile flags.

### 2.3 NPU Detection

| Platform | Current | Target |
|----------|---------|--------|
| Windows | WMI PnP entities (partial) | Add DirectML NPU detection (`DML_CREATE_DEVICE`), Intel NPU driver check |
| Linux | sysfs paths (mostly stub) | Add `/sys/devices/platform/` parse for AMD XDNA / Intel NPU / Qualcomm, check `libnpu` or `npu-smi` |
| macOS | **Missing** | Add Apple Neural Engine probe via `ANE` framework or `sysctl` |

### 2.4 CPU Feature Detection

Add SVE (Scalable Vector Extension) detection for ARM — currently only NEON is used:

```rust
// In detect_acceleration_mode, add:
#[cfg(target_arch = "aarch64")]
{
    // Check /proc/cpuinfo for "sve" on Linux, sysctl on macOS
    if sve_available() { return AccelerationMode::Sve; }
}
```

### 2.5 Vulkan Backend Detection

The `GpuBackend::Vulkan` variant exists but is never detected. Add:
- Linux: `vulkaninfo` or `libvulkan` probe
- Windows: Vulkan loader via `vkEnumeratePhysicalDevices`
- macOS: MoltenVK (rare but possible)

---

## Phase 3: Homogeneous Auto-Tuning API

**Problem:** Each subsystem (`HealthConfig`, `LoadBalanceConfig`, `PlanningTuning`, TCP transport) implements its own `autotuned()` method with different signatures and varying degrees of sophistication.

**Solution:** Create a single `AutoTuner` that derives all config from `SystemProfile`:

```rust
pub struct AutoTuner {
    pub health: HealthConfig,
    pub load_balance: LoadBalanceConfig,
    pub planning: PlanningTuning,
    pub tcp_transport: TcpTransportConfig,
    pub worker_pool: WorkerPoolConfig,
}

impl AutoTuner {
    pub fn from_system_profile(profile: &SystemProfile) -> Self;
    pub fn benchmark_tcp(&mut self) -> impl Future<Output = ()>;  // async benchmark
    pub fn save_cache(&self, path: &Path) -> Result<()>;
    pub fn load_cache(path: &Path) -> Option<Self>;
}
```

**Improvements over current autotune:**
- TCP autotune currently lives in `main.rs` as a CLI helper (`autotune_tcp_transport_config()`). Move it into `ghostlink-core` so it's usable by the API server directly.
- Add persistent cache (`~/.ghostlink/autotune.json`) that survives restarts, keyed by system fingerprint (CPU + GPU + RAM hash).
- Health/load balance autotuning currently only checks `AccelerationMode`. Also consider: number of GPUs (multi-GPU needs tighter bounds), NUMA zones, network interface type.

---

## Phase 4: Heterogeneous & Multi-Device Runtime Detection

### 4.1 Multiple GPU Detection

Today's `RuntimeProfile.node_resources` reports a single GPU. Change to:

```rust
pub struct SystemProfile {
    pub gpus: Vec<GpuInfo>,
    // ...
}
```

And add a `primary_gpu: Option<usize>` index. The planner can then distribute across multiple GPUs on the same host.

### 4.2 Integrated GPU + Discrete GPU

Detect both iGPU and dGPU. Example: Intel Arc + NVIDIA RTX on the same machine. The `AutoTuner` should prefer dGPU for inference but keep iGPU as a fallback.

### 4.3 Runtime Priority Chain

Replace the linear priority list in `RuntimeDetector::detect_primary()` with a weighted scoring system:

```rust
pub struct RuntimeScore {
    pub runtime: Runtime,
    pub score: f32,
    pub reasons: Vec<String>,
}
```

Factors: memory bandwidth, VRAM size, driver version, temperature, power mode.

### 4.4 MPS (Multi-Process Service) Detection

Check if NVIDIA MPS daemon is running (`nvidia-cuda-mps-control -d`) and adjust worker counts accordingly.

---

## Phase 5: Dynamic Re-Detection & Hot-Plug

**Problem:** All detection happens at startup. If a GPU is plugged in or a runtime is installed mid-session, it's missed.

**Solution:** Add an OS-level watcher:

```rust
pub struct SystemProfileWatcher {
    inner: Arc<Mutex<SystemProfile>>,
    change_tx: broadcast::Sender<SystemProfileChange>,
}

impl SystemProfileWatcher {
    pub fn watch() -> Self;
    // Uses inotify (Linux), ReadDirectoryChanges (Windows), FSEvents (macOS)
    // to detect changes to /dev/dri/, /proc/meminfo, driver installs
}
```

The health monitor, load balancer, and planner subscribe to changes. When a new GPU appears, layers can be redistributed.

---

## Phase 6: Platform Integration Testing

Add a test suite that runs per-platform:

| Commit | Files Changed | Description |
|--------|--------------|-------------|
| 1 | `ghostlink-core/src/system_profile.rs` (new) | `SystemProfile` struct with platform-specific detection |
| 2 | `ghostlink-core/src/host.rs` | Fold into system_profile, add macOS/Windows memory, Vulkan, NPU |
| 3 | `ghost-link/src/runtime.rs` | Rewrite as SystemProfile consumer |
| 4 | `ghost-link/src/backend_registry.rs` | Rewrite as SystemProfile consumer |
| 5 | `ghostlink-core/src/autotune.rs` (new) | Unified AutoTuner with cache |
| 6 | `ghost-link/src/main.rs` | Remove ad-hoc TCP autotune, use AutoTuner |
| 7 | `ghostlink-core/src/health.rs` | Use AutoTuner for HealthConfig |
| 8 | `ghostlink-core/src/load_balance.rs` | Use AutoTuner for LoadBalanceConfig |
| 9 | `ghostlink-core/src/planning.rs` | Use AutoTuner for PlanningTuning |
| 10 | `ghostlink-core/src/watcher.rs` (new) | Dynamic re-detection watcher |
| 11 | Cross-platform CI tests | GitHub Actions matrix (ubuntu-latest, windows-latest, macos-latest) |

---

## Summary of Gaps Found

| Gap | Severity | Impact |
|-----|----------|--------|
| macOS system memory missing in host.rs | High | Wrong worker counts on macOS |
| Windows system memory missing in host.rs | High | Wrong worker counts on Windows |
| AMD GPUs mislabeled without `rocm` feature | Medium | Wrong backend selection on ROCm systems |
| NPU detection is Windows-only WMI, Linux is stub | Medium | Missed NPU accelerators on Linux |
| Detection logic duplicated in 3 files | Medium | Inconsistent results, maintenance burden |
| No Vulkan backend detection | Low | Vulkan variant is dead code |
| TCP autotune lives in CLI binary only | Medium | Not usable by API/daemon mode |
| No persistent autotune cache across restarts | Low | Repeated warmup after restart |
| No multi-GPU support | Low | Cannot leverage multiple GPUs on one host |
| No hot-plug / dynamic re-detection | Low | Must restart to detect new hardware |
| No SVE detection for ARM | Low | Missed optimization on Graviton/etc |
| No NUMA topology awareness | Low | Suboptimal memory pinning on multi-socket |
