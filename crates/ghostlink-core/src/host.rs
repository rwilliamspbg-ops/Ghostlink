//! Host resource detection and runtime auto-tuning.
//!
//! This module detects the local machine's available resources and derives a
//! conservative runtime profile for worker counts and acceleration strategy.

use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::protocol::NodeResources;

const FULL_PROBE_CACHE_TTL: Duration = Duration::from_secs(30);
const FAST_PROFILE_CACHE_TTL: Duration = Duration::from_secs(5);

static FULL_PROBE_CACHE: OnceLock<Mutex<Option<CachedProbeEntry>>> = OnceLock::new();
static FAST_PROFILE_CACHE: OnceLock<Mutex<Option<CachedRuntimeProfileEntry>>> = OnceLock::new();

/// Selected acceleration path for the current host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccelerationMode {
    /// GPU-backed execution is available.
    Gpu,
    /// AVX-512 optimized CPU path is preferred.
    Avx512,
    /// AVX2 optimized CPU path is preferred.
    Avx2,
    /// ARM NEON optimized CPU path is preferred.
    Neon,
    /// Generic scalar fallback.
    Generic,
}

/// Hardware probe mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProbeMode {
    /// Fast path: env, sysfs, and visible-device hints only.
    Fast,
    /// Full path: includes cached external probes such as `nvidia-smi` and `lspci`.
    Full,
}

impl ProbeMode {
    /// Human-readable label.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Fast => "fast",
            Self::Full => "full",
        }
    }
}

impl AccelerationMode {
    /// Human-readable label for display.
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

/// Auto-tuned runtime profile for the local host.
#[derive(Clone, Debug)]
pub struct RuntimeProfile {
    /// Node resources advertised to the cluster.
    pub node_resources: NodeResources,
    /// Logical CPU cores detected on the host.
    pub logical_cores: usize,
    /// Recommended worker count for network and scheduling tasks.
    pub recommended_workers: usize,
    /// Preferred acceleration path.
    pub acceleration_mode: AccelerationMode,
    /// Whether AF_XDP can plausibly be used on this host.
    pub xdp_supported: bool,
    /// Best-effort hardware probe source.
    pub detection_source: String,
    /// Probe mode used to build this profile.
    pub probe_mode: ProbeMode,
}

impl RuntimeProfile {
    /// Generate a display-oriented summary.
    pub fn summary(&self) -> String {
        format!(
            "Host Runtime Profile\n\
             ====================\n\
             Node ID: {}\n\
             Logical cores: {}\n\
             Recommended workers: {}\n\
             System memory: {:.1} GB\n\
             GPU VRAM: {:.1} GB\n\
             Compute capability: {}\n\
             GPU: {}\n\
             Acceleration: {}\n\
             XDP support: {}\n\
             Detection source: {}\n\
             Probe mode: {}\n",
            self.node_resources.id,
            self.logical_cores,
            self.recommended_workers,
            self.node_resources.system_memory_gb,
            self.node_resources.vram_gb,
            if self.node_resources.compute_capability.is_empty() {
                "cpu"
            } else {
                &self.node_resources.compute_capability
            },
            self.node_resources
                .gpu_name
                .as_deref()
                .unwrap_or("Not detected"),
            self.acceleration_mode.as_str(),
            if self.xdp_supported {
                "available"
            } else {
                "unavailable"
            },
            self.detection_source,
            self.probe_mode.as_str()
        )
    }
}

#[derive(Clone, Debug, Default)]
struct GpuProbeResult {
    gpu_name: Option<String>,
    vram_gb: Option<f32>,
    compute_capability: Option<String>,
    detection_source: Option<String>,
}

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

/// Detect the local runtime profile for the host.
pub fn detect_runtime_profile(node_id: impl Into<String>) -> RuntimeProfile {
    detect_runtime_profile_with_mode(node_id, ProbeMode::Fast)
}

/// Detect the local runtime profile for the host using a specific probe mode.
pub fn detect_runtime_profile_with_mode(
    node_id: impl Into<String>,
    probe_mode: ProbeMode,
) -> RuntimeProfile {
    let node_id = node_id.into();

    if let Some(mut profile) = load_cached_runtime_profile(probe_mode) {
        profile.node_resources.id = node_id;
        return profile;
    }

    let logical_cores = std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1);
    let system_memory_gb = detect_system_memory_gb().unwrap_or(0.0);
    let gpu_probe = detect_gpu_probe(probe_mode);
    let vram_gb = detect_gpu_vram_gb(gpu_probe.vram_gb).unwrap_or(0.0);
    let gpu_name = detect_gpu_name(gpu_probe.gpu_name.clone());
    let compute_capability = detect_compute_capability(gpu_probe.compute_capability.clone());
    let acceleration_mode = detect_acceleration_mode(vram_gb, gpu_name.as_deref());
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

/// Detect the current host's node resources.
pub fn detect_local_node_resources(node_id: impl Into<String>) -> NodeResources {
    detect_runtime_profile(node_id).node_resources
}

/// Detect the local runtime profile using the full probe path.
pub fn detect_runtime_profile_full(node_id: impl Into<String>) -> RuntimeProfile {
    detect_runtime_profile_with_mode(node_id, ProbeMode::Full)
}

fn detect_system_memory_gb() -> Option<f32> {
    if let Some(value) = env_f32("GHOSTLINK_SYSTEM_MEMORY_GB") {
        return Some(value.max(0.0));
    }

    let meminfo = fs::read_to_string("/proc/meminfo").ok()?;
    let line = meminfo.lines().find(|line| line.starts_with("MemTotal:"))?;
    let kb = line
        .split_whitespace()
        .nth(1)
        .and_then(|value| value.parse::<f32>().ok())?;
    Some(kb / 1024.0 / 1024.0)
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
            ProbeMode::Full => detect_full_gpu_probe_cached(),
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
    let drm_root = Path::new("/sys/class/drm");
    let entries = fs::read_dir(drm_root).ok()?;

    for entry in entries.flatten() {
        let path = entry.path();
        let Some(file_name) = path.file_name() else {
            continue;
        };
        if !file_name.to_string_lossy().starts_with("card") {
            continue;
        }

        let device_path = path.join("device");
        if !device_path.exists() {
            continue;
        }

        let gpu_name = fs::read_to_string(device_path.join("product_name"))
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .or_else(|| read_sysfs_model_name(&device_path));
        let vram_gb = fs::read_to_string(device_path.join("mem_info_vram_total"))
            .ok()
            .and_then(|value| value.trim().parse::<f32>().ok())
            .map(|bytes| bytes / 1024.0 / 1024.0 / 1024.0);
        let compute_capability = gpu_name
            .as_ref()
            .map(|name| infer_compute_capability_from_name(name));

        if gpu_name.is_some() || vram_gb.is_some() {
            return Some(GpuProbeResult {
                gpu_name,
                vram_gb,
                compute_capability,
                detection_source: Some(String::from("sysfs")),
            });
        }
    }

    None
}

fn read_sysfs_model_name(device_path: &Path) -> Option<String> {
    let vendor = fs::read_to_string(device_path.join("vendor")).ok()?;
    let device = fs::read_to_string(device_path.join("device")).ok()?;
    Some(format!(
        "{}:{}",
        vendor.trim().trim_start_matches("0x"),
        device.trim().trim_start_matches("0x")
    ))
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
    if let Some(probe) = cache
        .lock()
        .unwrap()
        .as_ref()
        .filter(|entry| entry.captured_at.elapsed() < FULL_PROBE_CACHE_TTL)
        .map(|entry| entry.probe.clone())
    {
        return Some(probe);
    }

    let probe = detect_nvidia_smi().or_else(detect_lspci_gpu)?;
    *cache.lock().unwrap() = Some(CachedProbeEntry {
        captured_at: Instant::now(),
        probe: probe.clone(),
    });
    Some(probe)
}

fn load_cached_runtime_profile(probe_mode: ProbeMode) -> Option<RuntimeProfile> {
    if probe_mode != ProbeMode::Fast {
        return None;
    }

    let cache = FAST_PROFILE_CACHE.get_or_init(|| Mutex::new(None));
    let guard = cache.lock().ok()?;
    let entry = guard.as_ref()?;
    if entry.captured_at.elapsed() > FAST_PROFILE_CACHE_TTL {
        return None;
    }

    Some(entry.profile.clone())
}

fn store_cached_runtime_profile(profile: &RuntimeProfile) {
    if profile.probe_mode != ProbeMode::Fast {
        return;
    }

    let cache = FAST_PROFILE_CACHE.get_or_init(|| Mutex::new(None));
    if let Ok(mut guard) = cache.lock() {
        *guard = Some(CachedRuntimeProfileEntry {
            captured_at: Instant::now(),
            profile: profile.clone(),
        });
    }
}

fn detect_nvidia_smi() -> Option<GpuProbeResult> {
    let output = Command::new("nvidia-smi")
        .args([
            "--query-gpu=name,memory.total,compute_cap",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    parse_nvidia_smi_csv(&String::from_utf8_lossy(&output.stdout)).map(|mut probe| {
        probe.detection_source = Some(String::from("nvidia-smi"));
        probe
    })
}

fn detect_lspci_gpu() -> Option<GpuProbeResult> {
    let output = Command::new("lspci").arg("-mm").output().ok()?;
    if !output.status.success() {
        return None;
    }

    parse_lspci_gpu(&String::from_utf8_lossy(&output.stdout)).map(|mut probe| {
        probe.detection_source = Some(String::from("lspci"));
        probe
    })
}

fn parse_nvidia_smi_csv(stdout: &str) -> Option<GpuProbeResult> {
    let line = stdout.lines().find(|line| !line.trim().is_empty())?;
    let mut parts = line.split(',').map(str::trim);
    let gpu_name = parts.next()?.to_string();
    let memory_mib = parts.next()?.parse::<f32>().ok()?;
    let compute_capability = parts.next().map(|value| value.to_string());

    Some(GpuProbeResult {
        gpu_name: Some(gpu_name),
        vram_gb: Some(memory_mib / 1024.0),
        compute_capability,
        detection_source: None,
    })
}

fn parse_lspci_gpu(stdout: &str) -> Option<GpuProbeResult> {
    let line = stdout.lines().find(|line| {
        line.contains("VGA compatible controller") || line.contains("3D controller")
    })?;
    let quoted_fields: Vec<_> = line
        .split('"')
        .enumerate()
        .filter_map(|(index, segment)| {
            if index % 2 == 1 {
                let trimmed = segment.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
            None
        })
        .collect();
    let description = quoted_fields
        .get(2)
        .cloned()
        .or_else(|| quoted_fields.last().cloned())
        .or_else(|| Some(line.trim().to_string()));

    Some(GpuProbeResult {
        gpu_name: description,
        vram_gb: None,
        compute_capability: Some(String::from("gpu")),
        detection_source: None,
    })
}

fn infer_compute_capability_from_name(name: &str) -> String {
    let lowered = name.to_ascii_lowercase();
    if lowered.contains("rtx 50") || lowered.contains("rtx50") {
        String::from("9.0")
    } else if lowered.contains("rtx 40") || lowered.contains("rtx40") {
        String::from("8.9")
    } else if lowered.contains("rtx 30") || lowered.contains("rtx30") {
        String::from("8.6")
    } else if lowered.contains("arc") {
        String::from("xe")
    } else if lowered.contains("radeon") || lowered.contains("amd") {
        String::from("rocm")
    } else {
        String::from("gpu")
    }
}

fn detect_acceleration_mode(vram_gb: f32, gpu_name: Option<&str>) -> AccelerationMode {
    if vram_gb > 0.0 || gpu_name.is_some() {
        return AccelerationMode::Gpu;
    }

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        if std::is_x86_feature_detected!("avx512f") {
            return AccelerationMode::Avx512;
        }
        if std::is_x86_feature_detected!("avx2") {
            return AccelerationMode::Avx2;
        }
    }

    #[cfg(any(target_arch = "arm", target_arch = "aarch64"))]
    {
        return AccelerationMode::Neon;
    }

    AccelerationMode::Generic
}

fn recommend_worker_count(
    logical_cores: usize,
    system_memory_gb: f32,
    vram_gb: f32,
    acceleration_mode: AccelerationMode,
) -> usize {
    let cores = logical_cores.max(1);
    let memory_bound = if system_memory_gb >= 1.0 {
        (system_memory_gb / 4.0).floor() as usize
    } else {
        1
    }
    .max(1);

    let reserved_core = cores.saturating_sub(1).max(1);
    let accelerator_bonus = match acceleration_mode {
        AccelerationMode::Gpu if vram_gb >= 16.0 => 2,
        AccelerationMode::Gpu => 1,
        AccelerationMode::Avx512 => 1,
        _ => 0,
    };

    reserved_core
        .min(memory_bound)
        .saturating_add(accelerator_bonus)
}

fn visible_device_hint() -> Option<String> {
    [
        "CUDA_VISIBLE_DEVICES",
        "NVIDIA_VISIBLE_DEVICES",
        "ROCR_VISIBLE_DEVICES",
        "HIP_VISIBLE_DEVICES",
    ]
    .iter()
    .find_map(|key| env_string(key))
    .filter(|value| !value.trim().is_empty() && value.trim() != "void" && value.trim() != "none")
}

fn env_f32(key: &str) -> Option<f32> {
    env::var(key).ok()?.parse::<f32>().ok()
}

fn env_string(key: &str) -> Option<String> {
    let value = env::var(key).ok()?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recommends_more_workers_for_gpu_hosts() {
        let cpu_workers = recommend_worker_count(8, 32.0, 0.0, AccelerationMode::Avx2);
        let gpu_workers = recommend_worker_count(8, 32.0, 24.0, AccelerationMode::Gpu);

        assert!(gpu_workers > cpu_workers);
    }

    #[test]
    fn worker_count_is_capped_by_memory_budget() {
        let workers = recommend_worker_count(32, 8.0, 0.0, AccelerationMode::Generic);
        assert_eq!(workers, 2);
    }

    #[test]
    fn summary_includes_tuning_fields() {
        let profile = RuntimeProfile {
            node_resources: NodeResources::new(
                "node-a",
                24.0,
                64.0,
                "8.9",
                Some(String::from("RTX4090")),
            ),
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

    #[test]
    fn parses_nvidia_smi_output() {
        let probe = parse_nvidia_smi_csv("RTX 4090, 24564, 8.9\n").unwrap();
        assert_eq!(probe.gpu_name.as_deref(), Some("RTX 4090"));
        assert_eq!(probe.compute_capability.as_deref(), Some("8.9"));
        assert!(probe.vram_gb.unwrap() > 23.0);
    }

    #[test]
    fn parses_lspci_gpu_output() {
        let probe = parse_lspci_gpu(
            "0000:00:02.0 \"VGA compatible controller\" \"Intel Corporation\" \"Arc A770\" -r01 \"Vendor\" \"Device\"\n",
        )
        .unwrap();
        assert!(probe.gpu_name.unwrap().contains("Arc A770"));
    }

    #[test]
    fn infers_compute_capability_from_gpu_name() {
        assert_eq!(infer_compute_capability_from_name("RTX 4090"), "8.9");
        assert_eq!(infer_compute_capability_from_name("Arc A770"), "xe");
    }

    #[test]
    fn fast_profile_cache_reuses_detected_shape() {
        let first = detect_runtime_profile_with_mode("node-a", ProbeMode::Fast);
        let second = detect_runtime_profile_with_mode("node-b", ProbeMode::Fast);

        assert_eq!(first.logical_cores, second.logical_cores);
        assert_eq!(first.acceleration_mode, second.acceleration_mode);
        assert_eq!(second.node_resources.id, "node-b");
    }
}
