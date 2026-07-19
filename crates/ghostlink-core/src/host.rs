//! Host resource detection and runtime auto-tuning.
//!
//! This module detects the local machine's available resources and derives a
//! conservative runtime profile for worker counts and acceleration strategy.
//!
//! Most detection logic now delegates to [`crate::system_profile::SystemProfile`].

use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::protocol::NodeResources;

const FAST_PROFILE_CACHE_TTL: Duration = Duration::from_secs(5);

static FAST_PROFILE_CACHE: OnceLock<Mutex<Option<CachedRuntimeProfileEntry>>> = OnceLock::new();

/// Selected acceleration path for the current host.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum AccelerationMode {
    /// GPU-backed execution is available.
    #[default]
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

/// Backend technology hint for the detected accelerator path.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GpuBackend {
    Cuda,
    Rocm,
    Vulkan,
    Directml,
    Metal,
    Npu,
    Cpu,
}

impl GpuBackend {
    /// Human-readable backend label.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cuda => "cuda",
            Self::Rocm => "rocm",
            Self::Vulkan => "vulkan",
            Self::Directml => "directml",
            Self::Metal => "metal",
            Self::Npu => "npu",
            Self::Cpu => "cpu",
        }
    }
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
    /// Detected GPU backend technology.
    pub gpu_backend: GpuBackend,
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
             GPU Backend: {}\n\
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
            self.gpu_backend.as_str(),
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
///
/// Delegates to `SystemProfile` for unified detection logic.
pub fn detect_runtime_profile_with_mode(
    node_id: impl Into<String>,
    probe_mode: ProbeMode,
) -> RuntimeProfile {
    let node_id = node_id.into();

    if let Some(mut profile) = load_cached_runtime_profile(probe_mode) {
        profile.node_resources.id = node_id;
        return profile;
    }

    let sp = match probe_mode {
        ProbeMode::Fast => crate::system_profile::SystemProfile::detect_fast(),
        ProbeMode::Full => crate::system_profile::SystemProfile::detect(),
    };

    let mut profile: RuntimeProfile = sp.into();
    profile.node_resources.id = node_id;
    profile.probe_mode = probe_mode;

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

/// Infer GPU compute capability from its marketing name.
pub fn infer_compute_capability_from_name(name: &str) -> String {
    let lowered = name.to_ascii_lowercase();
    if lowered.contains("rtx 50") || lowered.contains("rtx50") || lowered.contains("b100") || lowered.contains("b200") {
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
        if cfg!(feature = "rocm") { String::from("rocm") } else { String::from("gpu") }
    } else if lowered.contains("intel") || lowered.contains("iris") || lowered.contains("uhd") {
        String::from("xe")
    } else if lowered.contains("apple") || lowered.contains("m1") || lowered.contains("m2") || lowered.contains("m3") || lowered.contains("m4") {
        String::from("apple_metal")
    } else {
        String::from("gpu")
    }
}

/// Whether the current host is Apple Silicon (ARM64 Mac).
#[cfg(target_os = "macos")]
pub fn is_apple_silicon() -> bool {
    std::process::Command::new("sysctl")
        .arg("hw.optional.arm64")
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).contains("1"))
        .unwrap_or(false)
}

/// Whether the current host is Apple Silicon (ARM64 Mac).
#[cfg(not(target_os = "macos"))]
pub fn is_apple_silicon() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

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
            gpu_backend: GpuBackend::Cuda,
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
    fn infers_compute_capability_from_gpu_name() {
        assert_eq!(infer_compute_capability_from_name("RTX 4090"), "8.9");
        assert_eq!(infer_compute_capability_from_name("Arc A770"), "xe");
        #[cfg(feature = "rocm")]
        {
            assert_eq!(
                infer_compute_capability_from_name("AMD Radeon RX 7900 XTX"),
                "rocm"
            );
            assert_eq!(
                infer_compute_capability_from_name("AMD Instinct MI250"),
                "rocm"
            );
        }
        #[cfg(not(feature = "rocm"))]
        {
            assert_eq!(
                infer_compute_capability_from_name("AMD Radeon RX 7900 XTX"),
                "gpu"
            );
        }
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
