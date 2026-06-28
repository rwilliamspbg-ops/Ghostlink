//! Host resource detection, runtime auto-tuning, and discovery orchestration.
//!
//! This module detects the local machine's available resources for inference workloads,
//! and handles cluster formation with configurable network discovery options.

use std::env;
use std::fs;
use std::net::{Ipv4Addr, SocketAddrV4};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use thiserror::Error;

use crate::protocol::NodeResources;
use crate::xdp::{is_xdp_supported as xdp_is_supported};

const FULL_PROBE_CACHE_TTL: Duration = Duration::from_secs(30);
const FAST_PROFILE_CACHE_TTL: Duration = Duration::from_secs(5);

/// Configuration for discovery options including interface, ports, and fallbacks.
#[derive(Debug, Clone)]
pub struct JoinOptions {
    /// Network interface to bind discovery (e.g., "eth0" or IP)
    pub interface: Option<String>,
    /// Port range for TCP-based peer scanning if multicast fails
    pub port_range: std::ops::Range<u16>,
    /// Enable UDP multicast/broadcast fallbacks
    pub use_multicast: bool,
    /// Timeout in milliseconds for discovery operations
    pub timeout_ms: u64,
}

impl Default for JoinOptions {
    fn default() -> Self {
        Self {
            interface: None,  // Use all interfaces or multicast group
            port_range: 4500..4999, // Default TCP scan range if needed
            use_multicast: true,   // Enable UDP discovery by default
            timeout_ms: 3000,      // 3-second discovery timeout
        }
    }
}

/// Error types for discovery operations.
#[derive(Error, Debug)]
pub enum DiscoveryError {
    #[error("I/O error during discovery: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Socket creation failed: {0}")]
    SocketCreation(String),
    
    #[error("Discovery timeout after {}ms", .timeout_ms)]
    Timeout { timeout_ms: u64 },
    
    #[error("No peers discovered in network - check firewall or try --no-multicast to use TCP scan")]
    NoPeersFound,
}

/// Hardware profile cache entry.
#[derive(Clone, Debug)]
struct CachedProbeEntry {
    captured_at: Instant,
    probe: GpuProbeResult,
}

#[derive(Clone, Debug)]
struct CachedRuntimeProfileEntry {
    captured_at: Instant,
    profile: RuntimeProfile,
}

/// Selected acceleration path for the current host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccelerationMode {
    Gpu,
    Avx512,
    Avx2,
    Neon,
    Generic,
}

impl AccelerationMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Gpu => "GPU",
            Self::Avx512 => "AVX-512",
            Self::Avx2 => "AVX2",
            Self::Neon => "NEON",
            Self::Generic => "Generic CPU",
        }
    }
}

/// Hardware probe mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProbeMode {
    Fast,
    Full,
}

impl ProbeMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Fast => "fast",
            Self::Full => "full",
        }
    }
}

/// Auto-tuned runtime profile for the local host.
#[derive(Clone, Debug)]
pub struct RuntimeProfile {
    pub node_resources: NodeResources,
    pub logical_cores: usize,
    pub recommended_workers: usize,
    pub acceleration_mode: AccelerationMode,
    pub xdp_supported: bool,
    pub detection_source: String,
    pub probe_mode: ProbeMode,
}

impl RuntimeProfile {
    pub fn summary(&self) -> String {
        format!(
            "Host Runtime Profile\n  ====================\n  Node ID: {}\n  Logical cores: {}\n  Recommended workers: {}\n  System memory: {:.1} GB\n  GPU VRAM: {:.1} GB\n  Compute capability: {}\n  GPU: {}\n  Acceleration: {}\n  XDP support: {}\n  Detection source: {}\n  Probe mode: {}\n",
            self.node_resources.id,
            self.logical_cores,
            self.recommended_workers,
            self.node_resources.system_memory_gb,
            self.node_resources.vram_gb,
            if self.node_resources.compute_capability.is_empty() { "cpu" } else { &self.node_resources.compute_capability },
            self.node_resources.gpu_name.as_deref().unwrap_or("Not detected"),
            self.acceleration_mode.as_str(),
            if self.xdp_supported { "available" } else { "unavailable" },
            self.detection_source,
            self.probe_mode.as_str()
        )
    }
}

pub fn detect_runtime_profile(node_id: impl Into<String>) -> RuntimeProfile {
    detect_runtime_profile_with_mode(node_id, ProbeMode::Fast)
}

pub fn detect_runtime_profile_with_mode(
    node_id: impl Into<String>,
    probe_mode: ProbeMode,
) -> RuntimeProfile {
    let node_id = node_id.into();

    if let Some(mut profile) = load_cached_runtime_profile(probe_mode) {
        profile.node_resources.id = node_id;
        return profile;
    }

    // Detect hardware characteristics
    let logical_cores = std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1);
    
    let system_memory_gb = detect_system_memory_gb().unwrap_or(0.0);
    let gpu_probe = detect_gpu_probe(probe_mode);
    let vram_gb = detect_gpu_vram_gb(gpu_probe.vram_gb).unwrap_or(0.0);
    let gpu_name = detect_gpu_name(gpu_probe.gpu_name.clone());
    let compute_capability = detect_compute_capability(gpu_probe.compute_capability.clone());
    
    // Determine acceleration mode based on detected hardware
    let acceleration_mode = detect_acceleration_mode(
        vram_gb,
        gpu_name.as_deref(),
        Some(compute_capability.as_str()),
    );
    
    // Check XDP support (Linux-only)
    let xdp_supported = cfg!(target_os = "linux");
    
    let detection_source = gpu_probe
        .detection_source
        .unwrap_or_else(|| String::from("cpu-probe"));

    let node_resources = NodeResources::new(
        node_id,
        vram_gb,
        system_memory_gb,
        compute_capability,
        gpu_name,
    );

    // Recommend worker count based on hardware profile
    let recommended_workers = recommend_worker_count(
        logical_cores,
        node_resources.system_memory_gb,
        node_resources.vram_gb,
        acceleration_mode,
    );

    let profile = RuntimeProfile {
        node_resources,
        logical_cores,
        recommended_workers,
        acceleration_mode,
        xdp_supported,
        detection_source,
        probe_mode,
    };

    store_cached_runtime_profile(&profile);
    profile
}

pub fn detect_local_node_resources(node_id: impl Into<String>) -> NodeResources {
    detect_runtime_profile(node_id).node_resources
}

#[derive(Clone, Debug)]
struct GpuProbeResult {
    gpu_name: Option<String>,
    vram_gb: Option<f32>,
    compute_capability: Option<String>,
    detection_source: Option<String>,
}

fn detect_system_memory_gb() -> Option<f32> {
    if let Some(value) = env_f32("GHOSTLINK_SYSTEM_MEMORY_GB") {
        return Some(value.max(0.0));
    }

    #[cfg(target_os = "linux")]
    {
        let meminfo = fs::read_to_string("/proc/meminfo").ok()?;
        if let Some(line) = meminfo.lines().find(|line| line.starts_with("MemTotal:")) {
            let kb = line
                .split_whitespace()
                .nth(1)
                .and_then(|value| value.parse::<f32>().ok())?;
            return Some(kb / 1024.0 / 1024.0);
        }
    }

    // Windows/Mac fallback: use visible environment hints or default
    env_f32("GHOSTLINK_SYSTEM_MEMORY_GB").or_else(|| {
        if cfg!(target_os = "macos") {
            Some(8.0)  // Typical Mac minimum for this workload
        } else if cfg!(windows) {
            Some(16.0)   // Default to conservative Windows estimate
        } else {
            None
        }
    })
}

fn detect_gpu_vram_gb(probed_value: Option<f32>) -> Option<f32> {
    env_f32("GHOSTLINK_VRAM_GB").or(probed_value)
}

fn detect_gpu_name(probed_value: Option<String>) -> Option<String> {
    env_string("GHOSTLINK_GPU_NAME")
        .or(probed_value)
        .or_else(|| visible_device_hint().map(|_| String::from("Detected GPU")))
}

fn detect_compute_capability(probed_value: Option<String>) -> String {
    env_string("GHOSTLINK_COMPUTE_CAPABILITY")
        .or(probed_value)
        .or_else(|| visible_device_hint().map(|_| String::from("gpu")))
        .unwrap_or_else(|| String::from("cpu"))
}

fn detect_gpu_probe(probe_mode: ProbeMode) -> GpuProbeResult {
    detect_env_probe()
        .or_else(detect_sysfs_gpu)
        .or_else(|| match probe_mode {
            ProbeMode::Fast => None,
            #[cfg(target_os = "linux")] 
            ProbeMode::Full => Some(detect_full_gpu_probe_cached()), // Linux only for full mode
            _ => None,
        })
        .or_else(detect_visible_hint_probe)
        .unwrap_or_default()
}

fn detect_env_probe() -> Option<GpuProbeResult> {
    let gpu_name = env_string("GHOSTLINK_GPU_NAME");
    let vram_gb = env_f32("GHOSTLINK_VRAM_GB");
    let compute_capability = env_string("GHOSTLINK_COMPUTE_CAPABILITY");

    if gpu_name.is_none() && vram_gb.is_none() && compute_capability.is_none() {
        return None;
    }

    Some(GpuProbeResult {
        gpu_name,
        vram_gb,
        compute_capability,
        detection_source: Some(String::from("env")),
    })
}

fn detect_sysfs_gpu() -> Option<GpuProbeResult> {
    #[cfg(target_os = "linux")]
    return if let Ok(entries) = std::fs::read_dir("/sys/class/drm") {
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(file_name) = path.file_name() else { continue; };
            if !file_name.to_string_lossy().starts_with("card") { continue; }

            let device_path = path.join("device");
            if !device_path.exists() { continue; }

            let gpu_name_result = std::fs::read_to_string(device_path.join("product_name"))
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty());
            
            // Try to extract VRAM from mem_info_vram_total (in KiB)
            let vram_kb_result = std::fs::read_to_string(device_path.join("mem_info_vram_total"))
                .ok()
                .and_then(|v| v.trim().parse::<f32>().ok())
                .map(|bytes| bytes as f32 / 1024.0);

            if let Some(gpu_name) = gpu_name_result {
                return Some(GpuProbeResult {
                    gpu_name: Some(gpu_name),
                    vram_gb: vram_kb_result.map(|v| v / 1024.0), // Convert KiB -> GiB
                    compute_capability: None, 
                    detection_source: Some(String::from("sysfs")),
                });
            }
        }
    };

    #[cfg(not(target_os = "linux"))]
    return None;
    
    None
}

fn detect_visible_hint_probe() -> Option<GpuProbeResult> {
    visible_device_hint().map(|_| GpuProbeResult {
        gpu_name: Some(String::from("Detected GPU")),
        vram_gb: None,
        compute_capability: Some(String::from("gpu")),
        detection_source: Some(String::from("visible-device")),
    })
}

fn detect_full_gpu_probe_cached() -> Option<GpuProbeResult> {
    let cache = FULL_PROBE_CACHE.get_or_init(|| Mutex::new(None));
    
    if let Some(probe) = cache.lock().unwrap().as_ref()? 
        .filter(|entry| entry.captured_at.elapsed() < FULL_PROBE_CACHE_TTL) {
            return Some(probe.clone());
    }

    // Try external probes (Linux only for full mode)
    #[cfg(target_os = "linux")]
    let probe_result: Option<GpuProbeResult> = 
        detect_nvidia_smi().or_else(detect_lspci_gpu);
    
    #[cfg(not(target_os = "linux"))]
    let probe_result: Option<GpuProbeResult> = None;

    if let Some(probe) = &probe_result {
        *cache.lock().unwrap() = Some(CachedProbeEntry {
            captured_at: Instant::now(),
            probe: probe.clone(),
        });
    }
    
    probe_result
}

fn detect_nvidia_smi() -> Option<GpuProbeResult> {
    let output = Command::new("nvidia-smi")
        .args(["--query-gpu=name,memory.total,compute_cap", "--format=csv,noheader,nounits"])
        .output()
        .ok()?;
    
    if !output.status.success() { return None; }

    parse_nvidia_smi_csv(&String::from_utf8_lossy(&output.stdout))
}

fn detect_lspci_gpu() -> Option<GpuProbeResult> {
    let output = Command::new("lspci").arg("-mm").output().ok()?;
    
    if !output.status.success() { return None; }

    parse_lspci_gpu(&String::from_utf8_lossy(&output.stdout))
}

fn parse_nvidia_smi_csv(stdout: &str) -> Option<GpuProbeResult> {
    let line = stdout.lines().find(|line| !line.trim().is_empty())?;
    
    Some(GpuProbeResult {
        gpu_name: None, // Will be populated from first field if available
        vram_gb: None,  // nvidia-smi reports memory in MiB - conversion needed
        compute_capability: None,
        detection_source: None,
    })
}

fn parse_lspci_gpu(stdout: &str) -> Option<GpuProbeResult> {
    Some(GpuProbeResult {
        gpu_name: stdout.to_string(),
        vram_gb: None,
        compute_capability: Some(String::from("gpu")),
        detection_source: None,
    })
}

fn infer_compute_capability_from_name(name: &str) -> String {
    let lowered = name.to_ascii_lowercase();
    
    if lowered.contains("rtx 50") || lowered.contains("rtx5090") || lowered.contains("rtx5080") { "12.0" } else if 
        lowered.contains("rtx 40") || lowered.contains("rtx4090") || lowered.contains("rtx4080") { "8.9" }
    else if lowered.contains("rtx 30") || lowered.contains("rtx3090") || lowered.contains("rtx3060") { "8.6" } 
    else if lowered.contains("arc a770") || lowered.contains("intel arc ") { String::from("xe") }
    else if lowered.contains("radeon") || lowered.contains("amd radeon rx") { String::from("rocm") }
    else { "gpu".to_string() }
}

fn detect_acceleration_mode(
    vram_gb: f32,
    gpu_name: Option<&str>,
    compute_capability: Option<&str>,
) -> AccelerationMode {
    
    // Check for strong GPU signal first (VRAM > 0 is a good indicator of real hardware)
    if has_strong_gpu_signal(vram_gb, gpu_name, compute_capability) {
        return AccelerationMode::Gpu;
    }

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        // Check CPU feature flags for vector extensions
        if std::is_x86_feature_detected!("avx512f") {
            return AccelerationMode::Avx512;
        } else if std::is_x86_feature_detected!("avx2") {
            return AccelerationMode::Avx2;
        }
    }

    #[cfg(any(target_arch = "arm", target_arch = "aarch64"))]
    {
        // ARM devices typically use NEON for vector operations
        return AccelerationMode::Neon;
    }

    AccelerationMode::Generic
}

fn has_strong_gpu_signal(
    vram_gb: f32,
    gpu_name: Option<&str>,
    compute_capability: Option<&str>,
) -> bool {
    
    // VRAM > 0 strongly indicates real GPU hardware (not a CPU-only system)
    if vram_gb >= 1.0 { return true; }

    let lowered_cap = |s| s.map(|x| x.trim().to_ascii_lowercase());
    
    // Check compute capability hint for specific GPU mentions
    if let Some(capability) = compute_capability { 
        let lowered = lowered_cap(Some(capability));
        
        // Specific non-generic indicators (filter out placeholders like "cpu", "gpu")
        if !lowered.is_empty() && 
           !(lowered == "cpu" || lowered == "gpu") { return true; }
    }

    // Check GPU name field for known GPU manufacturers/models
    if let Some(name) = gpu_name {
        let lowered = name.trim().to_ascii_lowercase();
        
        if !is_placeholder_gpu_signal(&lowered) { 
            // Known GPU manufacturer/model keywords indicate real hardware
            return ["nvidia", "geforce", "rtx", "tesla", "quadro", 
                    "radeon", "amd", "intel arc", "apple m"].iter()
                .any(|token| lowered.contains(token));
        }
    }

    false
}

fn is_placeholder_gpu_signal(name: &str) -> bool {
    [
        "", "detected gpu", "unknown", "<missing>", 
        "void", "none"
    ].contains(&name)
}

fn recommend_worker_count(
    logical_cores: usize,
    system_memory_gb: f32,
    vram_gb: f32,
    acceleration_mode: AccelerationMode,
) -> usize {
    
    let cores = logical_cores.max(1);
    
    // Memory-bounded worker count (8 GB per worker for token pipeline buffers)
    let memory_bound = if system_memory_gb >= 1.0 {
        ((system_memory_gb / 8.0).floor() as usize).max(1)
    } else {
        1
    };

    // Reserve one core for IO/network tasks (if we have more than 1 core)
    let reserved_core = cores.saturating_sub(1).max(1);
    
    // Acceleration bonus: GPU nodes get extra workers for parallelism
    let accelerator_bonus = match acceleration_mode {
        AccelerationMode::Gpu if vram_gb >= 4.0 => 2,     // Multi-GPU setups benefit from more workers
        AccelerationMode::Gpu => 1,                      // Single GPU gets one bonus worker
        _ => 0,                                          // CPU-only doesn't get bonus
    };

    reserved_core.min(memory_bound).saturating_add(accelerator_bonus)
}

fn visible_device_hint() -> Option<String> {
    [
        "CUDA_VISIBLE_DEVICES", 
        "NVIDIA_VISIBLE_DEVICES", 
        "ROCR_VISIBLE_DEVICES",
        "HIP_VISIBLE_DEVICES",
    ].iter().find_map(|key| env_string(key))
        .filter(|value| !value.trim().is_empty() && 
                value.trim() != "void" && value.trim() != "none")
}

fn detect_network_reachable() -> Result<bool, DiscoveryError> {
    // Simple connectivity check to an external endpoint (Google's DNS)
    let socket = match UdpSocket::bind("0.0.0.0:0") {
        Ok(s) => s,
        Err(_) => return Ok(false), 
    };
    
    #[cfg(target_os = "linux")]
    {
        // Try to connect (non-blocking) - this won't actually wait for DNS response
        match socket.connect("8.8.8.8:53") {
            Ok(()) => Ok(true),     
            Err(_) if true => Ok(env::var("GHOSTLINK_SKIP_NETWORK_CHECK").is_ok()), 
            _ => Ok(false)
        }
    } else {
        // Windows/Mac often restrict this check - assume reachable unless explicitly disabled
        Ok(!env::var("GHOSTLINK_DISABLE_NETWORK_CHECK")?.parse::<bool>().unwrap_or(true))
    }
}

fn env_f32(key: &str) -> Option<f32> {
    env::var(key).ok()?.parse::<f32>().ok()
}

fn env_string(key: &str) -> Option<String> {
    let value = env::var(key).ok()?;
    Some(value.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recommends_more_workers_for_gpu_hosts() {
        let cpu_profile = RuntimeProfile {
            node_resources: NodeResources::new("cpu-node".into(), 0.0, 32.0, "8.9", None),
            logical_cores: 16,
            recommended_workers: 4,
            acceleration_mode: AccelerationMode::Avx2,
            xdp_supported: false,
            detection_source: String::from("cpu"),
            probe_mode: ProbeMode::Fast,
        };

        let gpu_profile = RuntimeProfile {
            node_resources: NodeResources::new("gpu-node".into(), 24.0, 64.0, "8.9", None),
            logical_cores: 16,
            recommended_workers: 8, // GPU gets bonus worker for parallelism
            acceleration_mode: AccelerationMode::Gpu,
            xdp_supported: false,
            detection_source: String::from("env"),
            probe_mode: ProbeMode::Fast,
        };

        assert!(gpu_profile.recommended_workers > cpu_profile.recommended_workers);
    }

    #[test]
    fn test_worker_count_is_capped_by_memory_budget() {
        let workers = recommend_worker_count(32, 8.0, 16.0, AccelerationMode::Generic);
        
        // With 8GB RAM and no GPU: memory_bound = floor(8/8) = 1 worker (minimum)
        assert_eq!(workers, 4); // Reserved core + acceleration bonus considerations
    }

    #[test]
    fn test_node_resources_construction() {
        let resources = NodeResources::new("test-node".into(), 24.0, 64.0, "8.9", None);
        
        assert_eq!(resources.id, "test-node");
        assert!((resources.vram_gb - 24.0).abs() < 1e-5);
    }

    #[test]
    fn test_infer_compute_capability_from_name() {
        assert_eq!(infer_compute_capability_from_name("RTX 4090"), "8.9");
        assert_eq!(infer_compute_capability_from_name("Arc A770"), "xe");
        assert_eq!(infer_compute_capability_from_name(""), "gpu"); // Default fallback
    }

    #[test]
    fn test_placeholder_gpu_signal_detection() {
        let is_real = !is_placeholder_gpu_signal("");
        assert!(!is_real);
        
        let is_real = !is_placeholder_gpu_signal("RTX 4090"); 
        assert!(is_real); // "Detected GPU" would also return true (not a placeholder)
    }

    #[test]
    fn test_environment_variable_override() {
        env::set_var("GHOSTLINK_VRAM_GB", "16.0");
        
        let result = detect_gpu_vram_gb(None);
        assert_eq!(result, Some(16.0)); // Should pick up from environment
        
        env::remove_var("GHOSTLINK_VRAM_GB");
    }

    #[test]
    fn test_acceleration_mode_for_real_gpu() {
        let mode = detect_acceleration_mode(24.0, None, Some("8.9"));
        
        assert_eq!(mode, AccelerationMode::Gpu); // VRAM > 0 indicates real GPU
        
        env::set_var("GHOSTLINK_GPU_NAME", "NVIDIA GeForce RTX 4090");
        let mode_with_name = detect_acceleration_mode(0.0, None, Some("gpu")); 
        assert_eq!(mode_with_name, AccelerationMode::Generic); // Placeholder signals only
        
        env::remove_var("GHOSTLINK_GPU_NAME");
    }

    #[test]
    fn test_system_memory_detection() {
        env::set_var("GHOSTLINK_SYSTEM_MEMORY_GB", "64.0");
        
        let result = detect_system_memory_gb();
        assert_eq!(result, Some(64.0)); // Should pick up from environment
        
        env::remove_var("GHOSTLINK_SYSTEM_MEMORY_GB");
    }

    #[test]
    fn test_compute_capability_from_env() {
        env::set_var("GHOSTLINK_COMPUTE_CAPABILITY", "9.0");
        
        let result = detect_compute_capability(None);
        assert_eq!(result, "9.0"); // Should pick up from environment
        
        env::remove_var("GHOSTLINK_COMPUTE_CAPABILITY");
    }

    #[test]
    fn test_profile_summary_generation() {
        let profile = RuntimeProfile {
            node_resources: NodeResources::new("node-a".into(), 24.0, 64.0, "8.9", None),
            logical_cores: 16,
            recommended_workers: 10,
            acceleration_mode: AccelerationMode::Gpu,
            xdp_supported: true,
            detection_source: String::from("test"),
            probe_mode: ProbeMode::Fast,
        };

        let summary = profile.summary();
        
        assert!(summary.contains("Recommended workers: 10"));
        assert!(summary.contains("Acceleration: GPU"));
        assert!(summary.contains("Probe mode: fast"));
    }
}
