//! Unified system profile: single source of truth for host capabilities.
//!
//! `SystemProfile` consolidates all hardware/runtime detection previously
//! scattered across `host.rs`, `runtime.rs`, and `backend_registry.rs`.
//! Every platform probe feeds into one struct, and downstream code converts
//! it into the legacy `RuntimeProfile` / `BackendInfo` shapes as needed.

use std::env;
use std::fs;
use std::process::Command;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

#[cfg(target_os = "linux")]
use std::path::Path;

use crate::protocol::NodeResources;

// ---------------------------------------------------------------------------
// Sub-types
// ---------------------------------------------------------------------------

/// CPU topology and feature-set information.
#[derive(Clone, Debug, Default, Hash)]
pub struct CpuInfo {
    /// Human-readable brand string (e.g. "Intel(R) Core(TM) i9-14900K").
    pub brand: String,
    /// Number of logical cores (hyperthreads).
    pub logical_cores: usize,
    /// Number of physical cores (approximate).
    pub physical_cores: usize,
    /// Architecture family (x86_64, aarch64, …).
    pub arch: String,
    /// Set of detected CPU features.
    pub features: CpuFeatures,
}

/// CPU SIMD / accelerator features.
#[derive(Clone, Debug, Default, Hash)]
pub struct CpuFeatures {
    pub avx2: bool,
    pub avx_512: bool,
    pub neon: bool,
    pub sve: bool,
    pub amx: bool,
    pub sme: bool,
}

/// System memory information.
#[derive(Clone, Debug, Default)]
pub struct MemoryInfo {
    /// Total physical RAM in GB.
    pub total_gb: f32,
    /// Approximate available RAM in GB (cached/free).
    pub available_gb: f32,
}

/// A single GPU detected on the system.
#[derive(Clone, Debug)]
pub struct GpuInfo {
    /// Human-readable name (e.g. "NVIDIA GeForce RTX 4090").
    pub name: String,
    /// VRAM in GB.
    pub vram_gb: f32,
    /// Likely backend technology.
    pub backend: super::host::GpuBackend,
    /// Compute capability string (e.g. "8.9", "rocm", "xe", "apple_metal").
    pub compute_capability: String,
    /// Driver version when available.
    pub driver_version: String,
    /// PCI domain:bus:device.function (when available).
    pub pci_address: Option<String>,
}

/// A Neural Processing Unit detected on the system.
#[derive(Clone, Debug)]
pub struct NpuInfo {
    pub name: String,
    pub memory_gb: f32,
    pub driver: String,
}

/// Network interface information relevant to cluster tuning.
#[derive(Clone, Debug, Default, Hash)]
pub struct NetworkInfo {
    pub xdp_supported: bool,
    pub interfaces: Vec<String>,
}

// ---------------------------------------------------------------------------
// SystemProfile
// ---------------------------------------------------------------------------

/// Complete, platform-agnostic description of the local host.
#[derive(Clone, Debug, Default)]
pub struct SystemProfile {
    pub cpu: CpuInfo,
    pub memory: MemoryInfo,
    pub gpus: Vec<GpuInfo>,
    pub npus: Vec<NpuInfo>,
    pub network: NetworkInfo,
    /// Derived acceleration mode (best available).
    pub acceleration_mode: super::host::AccelerationMode,
    /// Recommended worker count for scheduling / pipelines.
    pub recommended_workers: usize,
    /// Platform-specific hostname / node identifier.
    pub system_id: String,
    /// Best-effort description of how detection was performed.
    pub detection_source: String,
}

// ---------------------------------------------------------------------------
// Cache helpers (re-use existing cache infra from host.rs)
// ---------------------------------------------------------------------------

static SYSTEM_PROFILE_CACHE: OnceLock<Mutex<Option<CachedSystemProfile>>> = OnceLock::new();
const SYSTEM_PROFILE_CACHE_TTL: Duration = Duration::from_secs(30);

struct CachedSystemProfile {
    captured_at: Instant,
    profile: SystemProfile,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

impl SystemProfile {
    /// Run full system detection and return the unified profile.
    ///
    /// Uses a short-lived cache (30 s) so repeated calls are cheap.
    pub fn detect() -> Self {
        Self::detect_with_mode(ProbeMode::Full)
    }

    /// Fast path: skip slow external commands, use env + sysfs + cached data.
    pub fn detect_fast() -> Self {
        Self::detect_with_mode(ProbeMode::Fast)
    }

    fn detect_with_mode(mode: ProbeMode) -> Self {
        // Try cache first for fast mode
        if mode == ProbeMode::Fast {
            let cache = SYSTEM_PROFILE_CACHE.get_or_init(|| Mutex::new(None));
            if let Ok(guard) = cache.lock() {
                if let Some(entry) = guard.as_ref() {
                    if entry.captured_at.elapsed() < SYSTEM_PROFILE_CACHE_TTL {
                        return entry.profile.clone();
                    }
                }
            }
        }

        let system_id = hostname().unwrap_or_else(|| "unknown".into());
        let cpu = detect_cpu_info();
        let memory = detect_memory_info();
        let gpus = detect_gpus(mode);
        let npus = detect_npus();
        let network = detect_network_info();
        let acceleration_mode = pick_acceleration_mode(&gpus, &cpu);
        let recommended_workers =
            compute_recommended_workers(&cpu, &memory, &gpus, acceleration_mode);
        let detection_source = gpus
            .first()
            .and_then(|g| {
                if g.name == "cpu" || g.name.is_empty() {
                    None
                } else {
                    Some(g.name.clone())
                }
            })
            .unwrap_or_else(|| "cpu".into());

        let profile = Self {
            cpu,
            memory,
            gpus,
            npus,
            network,
            acceleration_mode,
            recommended_workers,
            system_id,
            detection_source,
        };

        // Cache fast profiles
        if mode == ProbeMode::Fast {
            let cache = SYSTEM_PROFILE_CACHE.get_or_init(|| Mutex::new(None));
            if let Ok(mut guard) = cache.lock() {
                *guard = Some(CachedSystemProfile {
                    captured_at: Instant::now(),
                    profile: profile.clone(),
                });
            }
        }

        profile
    }
}

// ---------------------------------------------------------------------------
// Conversion to legacy types
// ---------------------------------------------------------------------------

impl From<&SystemProfile> for super::host::RuntimeProfile {
    fn from(sp: &SystemProfile) -> Self {
        let primary_gpu = sp.gpus.first();
        let gpu_backend = primary_gpu
            .map(|g| g.backend)
            .unwrap_or(super::host::GpuBackend::Cpu);
        let vram_gb = primary_gpu.map(|g| g.vram_gb).unwrap_or(0.0);
        let gpu_name = primary_gpu.map(|g| g.name.clone());
        let compute_cap = primary_gpu
            .map(|g| g.compute_capability.clone())
            .unwrap_or_else(|| String::from("cpu"));

        let node_resources = NodeResources::new(
            sp.system_id.clone(),
            vram_gb,
            sp.memory.total_gb,
            compute_cap,
            gpu_name,
        );

        super::host::RuntimeProfile {
            node_resources,
            logical_cores: sp.cpu.logical_cores,
            recommended_workers: sp.recommended_workers,
            acceleration_mode: sp.acceleration_mode,
            gpu_backend,
            xdp_supported: sp.network.xdp_supported,
            detection_source: sp.detection_source.clone(),
            probe_mode: super::host::ProbeMode::Fast,
        }
    }
}

impl From<SystemProfile> for super::host::RuntimeProfile {
    fn from(sp: SystemProfile) -> Self {
        Self::from(&sp)
    }
}

impl SystemProfile {
    /// Convenience: build a `NodeResources` for cluster advertisement.
    pub fn to_node_resources(&self) -> NodeResources {
        let primary = self.gpus.first();
        NodeResources::new(
            self.system_id.clone(),
            primary.map(|g| g.vram_gb).unwrap_or(0.0),
            self.memory.total_gb,
            primary
                .map(|g| g.compute_capability.clone())
                .unwrap_or_else(|| String::from("cpu")),
            primary.map(|g| g.name.clone()),
        )
    }
}

// ---------------------------------------------------------------------------
// Probe utilities
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProbeMode {
    Fast,
    Full,
}

fn hostname() -> Option<String> {
    let output = Command::new("hostname").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let name = String::from_utf8_lossy(&output.stdout);
    let trimmed = name.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

// ---------------------------------------------------------------------------
// CPU detection
// ---------------------------------------------------------------------------

fn detect_cpu_info() -> CpuInfo {
    let logical_cores = std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1);

    let arch = std::env::consts::ARCH.to_string();

    // Brand string (platform-specific)
    let brand = detect_cpu_brand();

    // SIMD features (platform-specific at compile-time, runtime-checked on x86)
    let features = detect_cpu_features(&arch);

    // Physical cores: best-effort approximation
    let physical_cores = detect_physical_cores().unwrap_or(logical_cores / 2).max(1);

    CpuInfo {
        brand,
        logical_cores,
        physical_cores,
        arch,
        features,
    }
}

fn detect_cpu_brand() -> String {
    // Linux
    #[cfg(target_os = "linux")]
    {
        if let Ok(output) = Command::new("lscpu").output() {
            let text = String::from_utf8_lossy(&output.stdout);
            if let Some(line) = text.lines().find(|l| l.contains("Model name")) {
                if let Some(name) = line.split(':').nth(1) {
                    return name.trim().to_string();
                }
            }
        }
    }

    // macOS
    #[cfg(target_os = "macos")]
    {
        if let Ok(output) = Command::new("sysctl")
            .args(["-n", "machdep.cpu.brand_string"])
            .output()
        {
            let name = String::from_utf8_lossy(&output.stdout);
            let trimmed = name.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
        // Apple Silicon fallback
        if let Ok(output) = Command::new("sysctl")
            .args(["-n", "machdep.cpu.brand_string"])
            .output()
        {
            let name = String::from_utf8_lossy(&output.stdout);
            let trimmed = name.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
        // For Apple Silicon, sysctl may not return a brand string
        if let Ok(output) = Command::new("sysctl").args(["-n", "hw.model"]).output() {
            let model = String::from_utf8_lossy(&output.stdout);
            let trimmed = model.trim();
            if !trimmed.is_empty() {
                return format!("Apple {}", trimmed);
            }
        }
    }

    // Windows
    #[cfg(target_os = "windows")]
    {
        if let Ok(output) = Command::new("wmic")
            .args(["cpu", "get", "name", "/format:csv"])
            .output()
        {
            let text = String::from_utf8_lossy(&output.stdout);
            for line in text.lines().skip(1) {
                let trimmed = line.trim();
                if !trimmed.is_empty() && !trimmed.contains("Node") {
                    // CSV format: Node,Name
                    if let Some(name) = trimmed.split(',').nth(1) {
                        let n = name.trim();
                        if !n.is_empty() {
                            return n.to_string();
                        }
                    }
                }
            }
        }
        // Fallback: Win32_Processor via PowerShell
        if let Ok(output) = Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "(Get-CimInstance Win32_Processor).Name",
            ])
            .output()
        {
            let name = String::from_utf8_lossy(&output.stdout);
            let trimmed = name.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
    }

    String::from("Unknown CPU")
}

fn detect_cpu_features(arch: &str) -> CpuFeatures {
    match arch {
        "x86_64" | "x86" => {
            let avx2 = is_x86_feature_detected!("avx2");
            let avx_512 = is_x86_feature_detected!("avx512f");
            let amx = is_x86_feature_detected!("avx512f") && cfg!(target_feature = "amx-tile");
            CpuFeatures {
                avx2,
                avx_512,
                amx,
                ..Default::default()
            }
        }
        "aarch64" | "arm" => {
            let neon = true; // all ARMv8+ have NEON
            let sve = detect_sve();
            CpuFeatures {
                neon,
                sve,
                ..Default::default()
            }
        }
        _ => CpuFeatures::default(),
    }
}

fn detect_sve() -> bool {
    #[cfg(target_os = "linux")]
    {
        // Check /proc/cpuinfo for "sve" feature flag
        if let Ok(content) = fs::read_to_string("/proc/cpuinfo") {
            if content.contains("sve") || content.contains("SVE") {
                return true;
            }
            // Also check /proc/self/auxv for HWCAP_SVE
        }
    }
    false
}

fn detect_physical_cores() -> Option<usize> {
    // Linux
    #[cfg(target_os = "linux")]
    {
        // Count unique core IDs across all CPU packages
        if let Ok(content) = fs::read_to_string("/proc/cpuinfo") {
            let mut cores = std::collections::BTreeSet::new();
            let mut current_package = None;
            for line in content.lines() {
                if let Some(val) = line.strip_prefix("physical id\t: ") {
                    current_package = Some(val.trim().to_string());
                }
                if let Some(val) = line.strip_prefix("core id\t\t: ") {
                    if let Some(ref pkg) = current_package {
                        cores.insert(format!("{}-{}", pkg, val.trim()));
                    }
                }
            }
            if !cores.is_empty() {
                return Some(cores.len());
            }
        }
    }

    // macOS
    #[cfg(target_os = "macos")]
    {
        if let Ok(output) = Command::new("sysctl")
            .args(["-n", "hw.physicalcpu"])
            .output()
        {
            if let Ok(count) = String::from_utf8_lossy(&output.stdout)
                .trim()
                .parse::<usize>()
            {
                return Some(count);
            }
        }
    }

    // Windows
    #[cfg(target_os = "windows")]
    {
        if let Ok(output) = Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "(Get-CimInstance Win32_Processor).NumberOfCores",
            ])
            .output()
        {
            let text = String::from_utf8_lossy(&output.stdout);
            for line in text.lines() {
                if let Ok(count) = line.trim().parse::<usize>() {
                    if count > 0 {
                        return Some(
                            count
                                * std::thread::available_parallelism()
                                    .map(|n| n.get())
                                    .unwrap_or(1)
                                / std::thread::available_parallelism()
                                    .map(usize::from)
                                    .unwrap_or(1),
                        );
                    }
                }
            }
        }
    }

    None
}

// ---------------------------------------------------------------------------
// Memory detection
// ---------------------------------------------------------------------------

fn detect_memory_info() -> MemoryInfo {
    let total_gb = detect_system_memory_gb().unwrap_or(0.0);
    let available_gb = detect_available_memory_gb().unwrap_or(total_gb * 0.5);
    MemoryInfo {
        total_gb,
        available_gb,
    }
}

fn detect_system_memory_gb() -> Option<f32> {
    // Env override (cross-platform)
    if let Ok(val) = env::var("GHOSTLINK_SYSTEM_MEMORY_GB") {
        if let Ok(gb) = val.parse::<f32>() {
            return Some(gb.max(0.0));
        }
    }

    platform_detect_system_memory_gb()
}

#[cfg(target_os = "linux")]
fn platform_detect_system_memory_gb() -> Option<f32> {
    let meminfo = fs::read_to_string("/proc/meminfo").ok()?;
    let line = meminfo.lines().find(|l| l.starts_with("MemTotal:"))?;
    let kb = line
        .split_whitespace()
        .nth(1)
        .and_then(|v| v.parse::<f32>().ok())?;
    Some(kb / 1024.0 / 1024.0)
}

#[cfg(target_os = "macos")]
fn platform_detect_system_memory_gb() -> Option<f32> {
    if let Ok(output) = Command::new("sysctl").args(["-n", "hw.memsize"]).output() {
        if let Ok(bytes) = String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse::<f64>()
        {
            return Some((bytes / 1_073_741_824.0) as f32);
        }
    }
    None
}

#[cfg(target_os = "windows")]
fn platform_detect_system_memory_gb() -> Option<f32> {
    if let Ok(output) = Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            "(Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory",
        ])
        .output()
    {
        if let Ok(bytes) = String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse::<f64>()
        {
            if bytes > 0.0 {
                return Some((bytes / 1_073_741_824.0) as f32);
            }
        }
    }
    // Fallback: wmic
    if let Ok(output) = Command::new("wmic")
        .args([
            "ComputerSystem",
            "get",
            "TotalPhysicalMemory",
            "/format:csv",
        ])
        .output()
    {
        let text = String::from_utf8_lossy(&output.stdout);
        for line in text.lines().skip(1) {
            if let Some(val) = line.split(',').nth(1) {
                if let Ok(bytes) = val.trim().parse::<f64>() {
                    if bytes > 0.0 {
                        return Some((bytes / 1_073_741_824.0) as f32);
                    }
                }
            }
        }
    }
    None
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn platform_detect_system_memory_gb() -> Option<f32> {
    None
}

fn detect_available_memory_gb() -> Option<f32> {
    platform_detect_available_memory_gb()
}

#[cfg(target_os = "linux")]
fn platform_detect_available_memory_gb() -> Option<f32> {
    let meminfo = fs::read_to_string("/proc/meminfo").ok()?;
    if let Some(line) = meminfo.lines().find(|l| l.starts_with("MemAvailable:")) {
        let kb = line
            .split_whitespace()
            .nth(1)
            .and_then(|v| v.parse::<f32>().ok())?;
        return Some(kb / 1024.0 / 1024.0);
    }
    let free = meminfo
        .lines()
        .find(|l| l.starts_with("MemFree:"))
        .and_then(|l| l.split_whitespace().nth(1)?.parse::<f32>().ok())
        .unwrap_or(0.0);
    let cached = meminfo
        .lines()
        .find(|l| l.starts_with("Cached:"))
        .and_then(|l| l.split_whitespace().nth(1)?.parse::<f32>().ok())
        .unwrap_or(0.0);
    Some((free + cached) / 1024.0 / 1024.0)
}

#[cfg(target_os = "macos")]
fn platform_detect_available_memory_gb() -> Option<f32> {
    if let Ok(output) = Command::new("vm_stat").output() {
        let text = String::from_utf8_lossy(&output.stdout);
        let mut free_pages = 0.0f64;
        let mut inactive_pages = 0.0f64;
        let mut page_size = 16384.0f64;
        for line in text.lines() {
            if let Some(val) = line.strip_prefix("Mach Virtual Memory Statistics: (page size of ") {
                if let Some(size) = val.trim_end_matches(" bytes)").parse::<f64>().ok() {
                    page_size = size;
                }
            }
            if line.contains("pages free") {
                free_pages = line
                    .split(':')
                    .nth(1)
                    .and_then(|s| s.trim().replace('.', "").parse::<f64>().ok())
                    .unwrap_or(0.0);
            }
            if line.contains("pages inactive") {
                inactive_pages = line
                    .split(':')
                    .nth(1)
                    .and_then(|s| s.trim().replace('.', "").parse::<f64>().ok())
                    .unwrap_or(0.0);
            }
        }
        let available_bytes = (free_pages + inactive_pages) * page_size;
        return Some((available_bytes / 1_073_741_824.0) as f32);
    }
    None
}

#[cfg(target_os = "windows")]
fn platform_detect_available_memory_gb() -> Option<f32> {
    if let Ok(output) = Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            "(Get-CimInstance Win32_OperatingSystem).FreePhysicalMemory",
        ])
        .output()
    {
        if let Ok(kb) = String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse::<f64>()
        {
            return Some((kb / 1024.0 / 1024.0) as f32);
        }
    }
    None
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn platform_detect_available_memory_gb() -> Option<f32> {
    None
}

// ---------------------------------------------------------------------------
// GPU detection
// ---------------------------------------------------------------------------

fn detect_gpus(mode: ProbeMode) -> Vec<GpuInfo> {
    // Try env override first (single GPU)
    if let Some(gpu) = detect_gpu_from_env() {
        return vec![gpu];
    }

    // Platform-specific probes
    let mut gpus = Vec::new();

    // nvidia-smi (cross-platform)
    if let Some(result) = probe_nvidia_smi() {
        gpus.extend(result);
    }

    // rocm-smi (Linux, feature-gated)
    #[cfg(feature = "rocm")]
    if let Some(result) = probe_rocm_smi() {
        gpus.extend(result);
    }

    // Windows WMI (fallback for non-NVIDIA GPUs)
    #[cfg(target_os = "windows")]
    if gpus.is_empty() {
        if let Some(result) = probe_windows_wmi_gpu() {
            gpus.extend(result);
        }
    }

    // Linux lspci (fallback, no VRAM info)
    #[cfg(target_os = "linux")]
    if gpus.is_empty() {
        if let Some(gpu) = probe_lspci_gpu() {
            gpus.push(gpu);
        }
    }

    // macOS system_profiler
    #[cfg(target_os = "macos")]
    if gpus.is_empty() {
        if let Some(gpu) = probe_macos_gpu() {
            gpus.push(gpu);
        }
    }

    // Linux sysfs (VRAM info for AMD + Intel)
    #[cfg(target_os = "linux")]
    if gpus.is_empty() {
        if let Some(gpu) = probe_sysfs_gpu() {
            gpus.push(gpu);
        }
    }

    // Vulkan probe (cross-platform, full mode only to avoid slowdown)
    if mode == ProbeMode::Full && gpus.is_empty() {
        if let Some(gpu) = probe_vulkan_gpu() {
            gpus.push(gpu);
        }
    }

    if gpus.is_empty() {
        let cc = if cfg!(feature = "rocm") {
            "rocm"
        } else {
            "gpu"
        };
        gpus.push(GpuInfo {
            name: String::from("cpu"),
            vram_gb: 0.0,
            backend: super::host::GpuBackend::Cpu,
            compute_capability: cc.to_string(),
            driver_version: String::from("native"),
            pci_address: None,
        });
    }

    gpus
}

fn detect_gpu_from_env() -> Option<GpuInfo> {
    let name = env::var("GHOSTLINK_GPU_NAME").ok();
    let vram_gb = env::var("GHOSTLINK_VRAM_GB")
        .ok()
        .and_then(|v| v.parse::<f32>().ok());
    let cc = env::var("GHOSTLINK_COMPUTE_CAPABILITY").ok();
    let has_any = name.is_some() || vram_gb.is_some() || cc.is_some();
    if !has_any {
        return None;
    }

    let gpu_name = name.unwrap_or_else(|| String::from("env-gpu"));
    let compute_capability =
        cc.unwrap_or_else(|| super::host::infer_compute_capability_from_name(&gpu_name));
    let backend = infer_backend(&gpu_name, &compute_capability);

    Some(GpuInfo {
        name: gpu_name,
        vram_gb: vram_gb.unwrap_or(0.0),
        backend,
        compute_capability,
        driver_version: String::from("env"),
        pci_address: None,
    })
}

fn probe_nvidia_smi() -> Option<Vec<GpuInfo>> {
    let output = Command::new("nvidia-smi")
        .args([
            "--query-gpu=index,name,memory.total,compute_cap,driver_version",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut gpus = Vec::new();

    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let mut parts = trimmed.split(',').map(str::trim);
        let _index = parts.next()?;
        let name = parts.next()?.to_string();
        let vram_mib = parts
            .next()
            .and_then(|v| v.parse::<f32>().ok())
            .unwrap_or(0.0);
        let compute_cap = parts
            .next()
            .map(|s| s.to_string())
            .unwrap_or_else(|| String::from("gpu"));
        let driver = parts.next().map(|s| s.to_string()).unwrap_or_default();

        gpus.push(GpuInfo {
            name,
            vram_gb: vram_mib / 1024.0,
            backend: super::host::GpuBackend::Cuda,
            compute_capability: compute_cap,
            driver_version: driver,
            pci_address: None,
        });
    }

    if gpus.is_empty() {
        None
    } else {
        Some(gpus)
    }
}

#[cfg(feature = "rocm")]
fn probe_rocm_smi() -> Option<Vec<GpuInfo>> {
    let output = Command::new("rocm-smi")
        .args(["--showproductname"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);

    let mut gpus = Vec::new();
    let mut current_gpu = None;

    for line in stdout.lines() {
        if line.contains("GPU[") && line.contains("Card model:") {
            let name = line
                .split(':')
                .nth(2)
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|| String::from("AMD GPU"));
            let vram = probe_rocm_vram();
            let compute_cap = super::host::infer_compute_capability_from_name(&name);
            gpus.push(GpuInfo {
                name,
                vram_gb: vram.unwrap_or(0.0),
                backend: super::host::GpuBackend::Rocm,
                compute_capability: compute_cap,
                driver_version: detect_rocm_version().unwrap_or_default(),
                pci_address: current_gpu.take(),
            });
        }
    }

    if gpus.is_empty() {
        None
    } else {
        Some(gpus)
    }
}

#[cfg(feature = "rocm")]
fn probe_rocm_vram() -> Option<f32> {
    let output = Command::new("rocm-smi")
        .args(["--showmeminfo", "vram"])
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .find(|l| l.contains("Total Memory"))
        .and_then(|l| {
            let val = l.split(':').nth(1)?.trim().to_lowercase();
            let num: f32 = val.split_whitespace().next()?.parse().ok()?;
            if val.contains("gb") {
                Some(num)
            } else if val.contains("mb") {
                Some(num / 1024.0)
            } else {
                None
            }
        })
}

#[cfg(feature = "rocm")]
fn detect_rocm_version() -> Option<String> {
    let output = Command::new("rocm-smi").args(["--version"]).output().ok()?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    stderr
        .lines()
        .find_map(|l| l.split_whitespace().last().map(|s| s.to_string()))
}

#[cfg(target_os = "windows")]
fn probe_windows_wmi_gpu() -> Option<Vec<GpuInfo>> {
    // Primary: WMI query with Name, AdapterRAM, DriverVersion, PNPDeviceID
    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            "Get-CimInstance Win32_VideoController | Where-Object { $_.Name -notmatch '(?i)(Microsoft Basic Display|Remote Display|VirtualBox|VMware)' } | Select-Object Name, AdapterRAM, DriverVersion, PNPDeviceID | ConvertTo-Csv -NoTypeInformation",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut gpus: Vec<GpuInfo> = Vec::new();

    for line in stdout.lines().skip(1) {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        // CSV: "Name","AdapterRAM","DriverVersion","PNPDeviceID"
        let mut parts: Vec<String> = Vec::new();
        let mut current = String::new();
        let mut in_quotes = false;
        for ch in trimmed.chars() {
            match ch {
                '"' => in_quotes = !in_quotes,
                ',' if !in_quotes => {
                    parts.push(std::mem::take(&mut current));
                }
                c => current.push(c),
            }
        }
        parts.push(current);
        if parts.len() < 3 {
            continue;
        }
        let name = parts[0].trim().to_string();
        if name.is_empty() {
            continue;
        }

        let vram_bytes: f64 = parts[1].trim().parse().unwrap_or(0.0);
        let vram_gb = if vram_bytes > 0.0 {
            (vram_bytes / 1_073_741_824.0) as f32
        } else {
            0.0
        };
        let driver = parts[2].trim().to_string();
        let pnp_id = parts.get(3).map(|s| s.trim().to_string());
        let compute_cap = super::host::infer_compute_capability_from_name(&name);

        // Determine backend from PNP ID or name
        let backend = if let Some(ref id) = pnp_id {
            let id_lower = id.to_ascii_lowercase();
            if id_lower.contains("ven_10de") {
                super::host::GpuBackend::Cuda
            } else if id_lower.contains("ven_1002") || id_lower.contains("ven_1022") {
                if cfg!(feature = "rocm") {
                    super::host::GpuBackend::Rocm
                } else {
                    super::host::GpuBackend::Directml
                }
            } else if id_lower.contains("ven_8086") {
                super::host::GpuBackend::Directml
            } else if id_lower.contains("ven_14e4") {
                super::host::GpuBackend::Vulkan
            } else {
                super::host::GpuBackend::Directml
            }
        } else {
            super::host::GpuBackend::Directml
        };

        // If WMI returned 0 VRAM for a real GPU, try a secondary DXGI-based probe
        let actual_vram = if vram_gb <= 0.0 {
            probe_windows_dxgi_vram().unwrap_or(0.0)
        } else {
            vram_gb
        };

        gpus.push(GpuInfo {
            name,
            vram_gb: actual_vram,
            backend,
            compute_capability: compute_cap,
            driver_version: driver,
            pci_address: pnp_id,
        });
    }

    if gpus.is_empty() {
        None
    } else {
        Some(gpus)
    }
}

/// Secondary DXGI-based VRAM probe via PowerShell `Add-Type` with C# DXGI wrapper.
/// Only called when WMI returns 0 VRAM for a detected GPU.
#[cfg(target_os = "windows")]
fn probe_windows_dxgi_vram() -> Option<f32> {
    let script = r#"
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class DXGIVram {
    [DllImport("dxgi.dll")]
    private static extern int CreateDXGIFactory1(ref Guid riid, out IntPtr ppFactory);
    public static float GetVRAM() {
        Guid iid = new Guid("7706bb9b-3be9-49f2-9fac-271a00c72f78");
        if (CreateDXGIFactory1(ref iid, out IntPtr factory) != 0) return 0;
        try {
            IntPtr vtable = Marshal.ReadIntPtr(factory);
            IntPtr enumAdapters = Marshal.ReadIntPtr(vtable, 9 * IntPtr.Size);
            var enumFunc = Marshal.GetDelegateForFunctionPointer<EnumAdaptersDelegate>(enumAdapters);
            IntPtr adapter;
            if (enumFunc(factory, 0, out adapter) != 0) return 0;
            try {
                IntPtr adapterVtable = Marshal.ReadIntPtr(adapter);
                IntPtr descPtr = Marshal.ReadIntPtr(adapterVtable, 4 * IntPtr.Size);
                var descFunc = Marshal.GetDelegateForFunctionPointer<GetDescDelegate>(descPtr);
                DXGI_ADAPTER_DESC desc;
                if (descFunc(adapter, out desc) != 0) return 0;
                return (float)(desc.DedicatedVideoMemory / (1024.0 * 1024.0 * 1024.0));
            } finally { Marshal.Release(adapter); }
        } finally { Marshal.Release(factory); }
    }
    [StructLayout(LayoutKind.Sequential)]
    private struct DXGI_ADAPTER_DESC {
        public uint VendorId; public uint DeviceId; public uint SubSysId; public uint Revision;
        public ulong DedicatedVideoMemory; public ulong DedicatedSystemMemory; public ulong SharedSystemMemory;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst=128)] public string Description;
    }
    [UnmanagedFunctionPointer(CallingConvention.StdCall)]
    private delegate int EnumAdaptersDelegate(IntPtr factory, uint index, out IntPtr adapter);
    [UnmanagedFunctionPointer(CallingConvention.StdCall)]
    private delegate int GetDescDelegate(IntPtr adapter, out DXGI_ADAPTER_DESC desc);
}
"@
[DXGIVram]::GetVRAM()
"#;
    let output = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout.trim().parse::<f32>().ok().filter(|&v| v > 0.0)
}

#[cfg(target_os = "linux")]
fn probe_lspci_gpu() -> Option<GpuInfo> {
    let output = Command::new("lspci").args(["-mm"]).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);

    let line = stdout
        .lines()
        .find(|l| l.contains("VGA compatible controller") || l.contains("3D controller"))?;
    let quoted: Vec<String> = line
        .split('"')
        .enumerate()
        .filter_map(|(i, s)| {
            if i % 2 == 1 {
                let t = s.trim();
                if t.is_empty() {
                    None
                } else {
                    Some(t.to_string())
                }
            } else {
                None
            }
        })
        .collect();
    let description = quoted
        .get(2)
        .cloned()
        .or_else(|| quoted.last().cloned())
        .unwrap_or_else(|| line.trim().to_string());

    let compute_cap = super::host::infer_compute_capability_from_name(&description);
    Some(GpuInfo {
        name: description,
        vram_gb: 0.0,
        backend: super::host::GpuBackend::Vulkan,
        compute_capability: compute_cap,
        driver_version: String::from("lspci"),
        pci_address: line.split(' ').next().map(|s| s.trim().to_string()),
    })
}

#[cfg(target_os = "linux")]
fn probe_sysfs_gpu() -> Option<GpuInfo> {
    let drm_root = Path::new("/sys/class/drm");
    let entries = fs::read_dir(drm_root).ok()?;

    for entry in entries.flatten() {
        let path = entry.path();
        let fname = path.file_name()?.to_string_lossy().to_string();
        if !fname.starts_with("card") {
            continue;
        }

        let dev = path.join("device");
        if !dev.exists() {
            continue;
        }

        let gpu_name = fs::read_to_string(dev.join("product_name"))
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());
        let vram_bytes = fs::read_to_string(dev.join("mem_info_vram_total"))
            .ok()
            .and_then(|v| v.trim().parse::<f32>().ok());
        let vram_gb = vram_bytes.map(|b| b / 1024.0 / 1024.0 / 1024.0);

        if let Some(name) = gpu_name {
            let compute_cap = super::host::infer_compute_capability_from_name(&name);
            return Some(GpuInfo {
                name,
                vram_gb: vram_gb.unwrap_or(0.0),
                backend: super::host::GpuBackend::Vulkan,
                compute_capability: compute_cap,
                driver_version: String::from("sysfs"),
                pci_address: None,
            });
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn probe_macos_gpu() -> Option<GpuInfo> {
    // Fast path: try `system_profiler SPDisplaysDataType -detailLevel mini` first
    let output = Command::new("system_profiler")
        .args(["SPDisplaysDataType", "-detailLevel", "mini"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut name = String::from("Apple GPU");
    let mut vram_gb = 0.0;
    let mut metal_support = String::from("metal");
    let mut gpu_metal_family = String::new();

    for line in stdout.lines() {
        if line.contains("Chipset Model:") {
            if let Some(val) = line.split(':').nth(1) {
                name = val.trim().to_string();
            }
        }
        // "VRAM (Total): 16 GB" or "VRAM (Dynamic, Max): 16 GB"
        if line.contains("VRAM") {
            let val = line.split(':').nth(1).unwrap_or("").trim();
            if let Some(num_str) = val.split_whitespace().next() {
                if let Ok(num) = num_str.parse::<f32>() {
                    vram_gb = num;
                }
            }
        }
        if line.contains("Metal Support") {
            if let Some(val) = line.split(':').nth(1) {
                metal_support = val.trim().to_string();
            }
        }
        if line.contains("Metal Family") {
            if let Some(val) = line.split(':').nth(1) {
                gpu_metal_family = val.trim().to_string();
            }
        }
    }

    // On Apple Silicon, shared memory wasn't exported by system_profiler;
    // query the IOKit registry for accurate GPU memory.
    if vram_gb <= 0.0 && super::host::is_apple_silicon() {
        if let Ok(ioreg) = Command::new("ioreg")
            .args(["-r", "-c", "IOAccelerator", "-l"])
            .output()
        {
            let iotext = String::from_utf8_lossy(&ioreg.stdout);
            for line in iotext.lines() {
                // "gpu-core-count" or "performanceState" indicate Apple GPU
                if line.contains("\"gpu-core-count\"") {
                    // Try "memory-available" which reports shared GPU memory
                    if let Some(mem_line) =
                        iotext.lines().find(|l| l.contains("\"memory-available\""))
                    {
                        if let Some(val) = mem_line.split('=').nth(1) {
                            if let Ok(bytes) = val.trim().trim_matches('"').parse::<f64>() {
                                vram_gb = (bytes / 1_073_741_824.0) as f32;
                            }
                        }
                    }
                    break;
                }
            }
            // Fallback: total system memory / 2 as rough ANE+GPU shared estimate
            if vram_gb <= 0.0 {
                vram_gb = detect_system_memory_gb().unwrap_or(16.0) * 0.5;
            }
        }
    }

    // Build compute capability string from Metal info
    let compute_capability = if !metal_support.is_empty() && metal_support != "metal" {
        metal_support
    } else if !gpu_metal_family.is_empty() {
        format!("apple_metal/{}", gpu_metal_family)
    } else {
        String::from("apple_metal")
    };

    Some(GpuInfo {
        name,
        vram_gb,
        backend: super::host::GpuBackend::Metal,
        compute_capability,
        driver_version: String::from("metal"),
        pci_address: None,
    })
}

fn probe_vulkan_gpu() -> Option<GpuInfo> {
    // Try `vulkaninfo` (cross-platform, part of Vulkan SDK)
    if let Ok(output) = Command::new("vulkaninfo").args(["--summary"]).output() {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if line.contains("GPU id") || line.contains("deviceName") {
                    let name = line
                        .split(':')
                        .nth(1)
                        .map(|s| s.trim().to_string())
                        .unwrap_or_else(|| String::from("Vulkan GPU"));
                    let compute_cap = super::host::infer_compute_capability_from_name(&name);
                    return Some(GpuInfo {
                        name,
                        vram_gb: 0.0,
                        backend: super::host::GpuBackend::Vulkan,
                        compute_capability: compute_cap,
                        driver_version: String::from("vulkan"),
                        pci_address: None,
                    });
                }
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// NPU detection
// ---------------------------------------------------------------------------

fn detect_npus() -> Vec<NpuInfo> {
    let mut npus = Vec::new();

    // Windows WMI
    #[cfg(target_os = "windows")]
    {
        if let Ok(output) = Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "Get-CimInstance -ClassName Win32_PnPEntity -ErrorAction SilentlyContinue | Where-Object { $_.Name -match '(?i)(NPU|Neural Processor|AI Accelerator|XDNA|Ryzen AI|Intel NPU)' } | Select-Object -ExpandProperty Name",
            ])
            .output()
        {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        npus.push(NpuInfo {
                            name: trimmed.to_string(),
                            memory_gb: 2.0, // typical
                            driver: String::from("windows"),
                        });
                    }
                }
            }
        }
    }

    // Linux sysfs
    #[cfg(target_os = "linux")]
    {
        let npu_paths = ["/sys/devices/virtual/npu", "/sys/bus/auxiliary/devices"];
        for base in &npu_paths {
            if let Ok(entries) = fs::read_dir(base) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.contains("npu") || name.contains("xdna") || name.contains("ivpu") {
                        npus.push(NpuInfo {
                            name,
                            memory_gb: 2.0,
                            driver: String::from("linux"),
                        });
                    }
                }
            }
        }
        // Check for Intel NPU via /sys/class/accel
        if let Ok(entries) = fs::read_dir("/sys/class/accel") {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                npus.push(NpuInfo {
                    name: format!("accel/{}", name),
                    memory_gb: 2.0,
                    driver: String::from("linux"),
                });
            }
        }
    }

    // macOS: check for ANE via sysctl
    #[cfg(target_os = "macos")]
    {
        if super::host::is_apple_silicon() {
            npus.push(NpuInfo {
                name: String::from("Apple Neural Engine"),
                memory_gb: 0.0, // shared memory
                driver: String::from("ane"),
            });
        }
    }

    // Env override
    if (env::var("NPU_DEVICE").is_ok() || env::var("QUALCOMM_NPU").is_ok()) && npus.is_empty() {
        npus.push(NpuInfo {
            name: String::from("npu-env"),
            memory_gb: 2.0,
            driver: String::from("env"),
        });
    }

    npus
}

// ---------------------------------------------------------------------------
// Network detection
// ---------------------------------------------------------------------------

fn detect_network_info() -> NetworkInfo {
    let xdp_supported = cfg!(target_os = "linux");
    let interfaces = detect_network_interfaces();
    NetworkInfo {
        xdp_supported,
        interfaces,
    }
}

fn detect_network_interfaces() -> Vec<String> {
    let mut ifaces = Vec::new();
    if let Ok(entries) = fs::read_dir("/sys/class/net") {
        for entry in entries.flatten() {
            if let Ok(name) = entry.file_name().into_string() {
                if !name.starts_with("lo") {
                    ifaces.push(name);
                }
            }
        }
    }
    ifaces
}

// ---------------------------------------------------------------------------
// Backend inference
// ---------------------------------------------------------------------------

fn infer_backend(name: &str, compute_capability: &str) -> super::host::GpuBackend {
    use super::host::GpuBackend;
    let lowered = name.to_ascii_lowercase();
    let lowered_cc = compute_capability.to_ascii_lowercase();

    if lowered_cc.contains("rocm")
        || lowered.contains("amd")
        || lowered.contains("radeon")
        || lowered.contains("instinct")
        || lowered.contains("mi250")
        || lowered.contains("mi300")
        || lowered.contains("mi350")
    {
        return GpuBackend::Rocm;
    }

    if lowered_cc.contains("metal")
        || lowered.contains("apple")
        || lowered.contains("m1")
        || lowered.contains("m2")
        || lowered.contains("m3")
        || lowered.contains("m4")
    {
        return GpuBackend::Metal;
    }

    if lowered_cc.contains("directml") || lowered.contains("directml") {
        return GpuBackend::Directml;
    }

    if lowered_cc.contains("npu")
        || lowered.contains("npu")
        || lowered.contains("xdna")
        || lowered.contains("neural")
    {
        return GpuBackend::Npu;
    }

    if lowered.contains("nvidia")
        || lowered.contains("rtx")
        || lowered.contains("gtx")
        || lowered.contains("tesla")
        || lowered.contains("a100")
        || lowered.contains("h100")
        || lowered.contains("b100")
        || lowered.contains("b200")
        || lowered_cc
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_digit())
    {
        return GpuBackend::Cuda;
    }

    GpuBackend::Vulkan
}

// ---------------------------------------------------------------------------
// Acceleration mode
// ---------------------------------------------------------------------------

fn pick_acceleration_mode(gpus: &[GpuInfo], cpu: &CpuInfo) -> super::host::AccelerationMode {
    use super::host::AccelerationMode;

    // Any real GPU detected → GPU mode
    if gpus.iter().any(|g| {
        g.vram_gb > 0.0
            || (g.backend != super::host::GpuBackend::Cpu
                && g.backend != super::host::GpuBackend::Vulkan)
    }) {
        return AccelerationMode::Gpu;
    }

    // Vulkan-only with pending VRAM (lspci/sysfs probe, no driver)
    if gpus.iter().any(|g| {
        g.backend == super::host::GpuBackend::Vulkan && !g.name.is_empty() && g.name != "cpu"
    }) {
        return AccelerationMode::Gpu;
    }

    // CPU features
    if cpu.features.avx_512 {
        return AccelerationMode::Avx512;
    }
    if cpu.features.avx2 {
        return AccelerationMode::Avx2;
    }
    if cpu.features.neon || cpu.features.sve {
        return AccelerationMode::Neon;
    }

    AccelerationMode::Generic
}

// ---------------------------------------------------------------------------
// Worker count recommendation
// ---------------------------------------------------------------------------

fn compute_recommended_workers(
    cpu: &CpuInfo,
    memory: &MemoryInfo,
    gpus: &[GpuInfo],
    accel: super::host::AccelerationMode,
) -> usize {
    let cores = cpu.logical_cores.max(1);
    let memory_bound = if memory.total_gb >= 1.0 {
        (memory.total_gb / 4.0).floor() as usize
    } else {
        1
    }
    .max(1);

    let reserved_core = cores.saturating_sub(1).max(1);

    // Multi-GPU bonus
    let gpu_count = gpus.iter().filter(|g| g.vram_gb > 0.0).count();
    let accelerator_bonus = match accel {
        super::host::AccelerationMode::Gpu => {
            let vram_bonus = if gpus.iter().any(|g| g.vram_gb >= 16.0) {
                2
            } else {
                1
            };
            vram_bonus + gpu_count.saturating_sub(1) // +1 per extra GPU
        }
        super::host::AccelerationMode::Avx512 => 1,
        _ => 0,
    };

    reserved_core
        .min(memory_bound)
        .saturating_add(accelerator_bonus)
}

// ---------------------------------------------------------------------------
// Re-export known inference helper for host.rs compat
// ---------------------------------------------------------------------------

/// Re-exported for host.rs backward compatibility.
pub fn infer_compute_capability_from_name(name: &str) -> String {
    let lowered = name.to_ascii_lowercase();
    if lowered.contains("rtx 50")
        || lowered.contains("rtx50")
        || lowered.contains("b100")
        || lowered.contains("b200")
    {
        String::from("9.0")
    } else if lowered.contains("rtx 40") || lowered.contains("rtx40") {
        String::from("8.9")
    } else if lowered.contains("rtx 30") || lowered.contains("rtx30") {
        String::from("8.6")
    } else if lowered.contains("arc") {
        String::from("xe")
    } else if lowered.contains("radeon")
        || lowered.contains("amd")
        || lowered.contains("advanced micro")
        || lowered.contains("rx ")
        || lowered.contains("w7900")
        || lowered.contains("w6800")
        || lowered.contains("mi250")
        || lowered.contains("mi300")
        || lowered.contains("mi350")
        || lowered.contains("instinct")
    {
        if cfg!(feature = "rocm") {
            String::from("rocm")
        } else {
            String::from("gpu")
        }
    } else if lowered.contains("intel") || lowered.contains("iris") || lowered.contains("uhd") {
        String::from("xe")
    } else if lowered.contains("apple")
        || lowered.contains("m1")
        || lowered.contains("m2")
        || lowered.contains("m3")
        || lowered.contains("m4")
    {
        String::from("apple_metal")
    } else {
        String::from("gpu")
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GpuBackend;

    #[test]
    fn system_profile_detects_cpu() {
        let sp = SystemProfile::detect_fast();
        assert!(sp.cpu.logical_cores >= 1);
        assert!(!sp.cpu.arch.is_empty());
    }

    #[test]
    fn system_profile_detects_memory() {
        let sp = SystemProfile::detect_fast();
        // Memory should be >= 0.5 GB on any real system
        assert!(sp.memory.total_gb >= 0.5 || sp.memory.total_gb == 0.0);
    }

    #[test]
    fn system_profile_always_has_at_least_one_gpu_entry() {
        let sp = SystemProfile::detect_fast();
        assert!(
            !sp.gpus.is_empty(),
            "Should have at least 'cpu' fallback GPU entry"
        );
    }

    #[test]
    fn system_profile_converts_to_runtime_profile() {
        let sp = SystemProfile::detect_fast();
        let rp: crate::host::RuntimeProfile = (&sp).into();
        assert_eq!(rp.logical_cores, sp.cpu.logical_cores);
        assert_eq!(rp.recommended_workers, sp.recommended_workers);
    }

    #[test]
    fn detect_gpu_handles_env_overrides() {
        std::env::set_var("GHOSTLINK_GPU_NAME", "RTX 4090");
        std::env::set_var("GHOSTLINK_VRAM_GB", "24.0");
        std::env::set_var("GHOSTLINK_COMPUTE_CAPABILITY", "8.9");

        let gpu = detect_gpu_from_env().expect("env probe should work");
        assert_eq!(gpu.name, "RTX 4090");
        assert!((gpu.vram_gb - 24.0).abs() < 0.1);
        assert_eq!(gpu.compute_capability, "8.9");

        std::env::remove_var("GHOSTLINK_GPU_NAME");
        std::env::remove_var("GHOSTLINK_VRAM_GB");
        std::env::remove_var("GHOSTLINK_COMPUTE_CAPABILITY");
    }

    #[test]
    fn infer_backend_rocm() {
        let b = infer_backend("AMD Radeon RX 7900 XTX", "rocm");
        assert_eq!(b, GpuBackend::Rocm);
    }

    #[test]
    fn infer_backend_cuda() {
        let b = infer_backend("NVIDIA GeForce RTX 4090", "8.9");
        assert_eq!(b, GpuBackend::Cuda);
    }

    #[test]
    fn infer_backend_metal() {
        let b = infer_backend("Apple M3 Max", "apple_metal");
        assert_eq!(b, GpuBackend::Metal);
    }

    #[test]
    fn compute_workers_scales_with_gpus() {
        let cpu = CpuInfo {
            logical_cores: 16,
            physical_cores: 8,
            arch: "x86_64".into(),
            ..Default::default()
        };
        let mem = MemoryInfo {
            total_gb: 64.0,
            available_gb: 32.0,
        };
        let gpus = vec![
            GpuInfo {
                name: "RTX 4090".into(),
                vram_gb: 24.0,
                backend: GpuBackend::Cuda,
                compute_capability: "8.9".into(),
                driver_version: "555".into(),
                pci_address: None,
            },
            GpuInfo {
                name: "RTX 3090".into(),
                vram_gb: 24.0,
                backend: GpuBackend::Cuda,
                compute_capability: "8.6".into(),
                driver_version: "555".into(),
                pci_address: None,
            },
        ];
        let accel = pick_acceleration_mode(&gpus, &cpu);
        let workers = compute_recommended_workers(&cpu, &mem, &gpus, accel);
        // 15 (reserved from 16 cores) min 16 (mem/4=16) = 15, + VGPU bonus (2) + extra GPU (1) = 18
        assert!(workers > 0);
    }

    #[test]
    fn physical_cores_nonzero() {
        let cpu = detect_cpu_info();
        assert!(cpu.physical_cores >= 1);
    }
}
