//! Backend registry for GPU/CPU auto-discovery and runtime switching
use serde::{Deserialize, Serialize};
use std::process::Command;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ComputeBackend {
    Rocm,
    Cuda,
    OneAPI,
    Metal,
    Directml,
    Vulkan,
    Npu,
    Cpu,
}

impl ComputeBackend {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Rocm => "rocm",
            Self::Cuda => "cuda",
            Self::OneAPI => "oneapi",
            Self::Metal => "metal",
            Self::Directml => "directml",
            Self::Vulkan => "vulkan",
            Self::Npu => "npu",
            Self::Cpu => "cpu",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "rocm" => Some(Self::Rocm),
            "cuda" => Some(Self::Cuda),
            "oneapi" => Some(Self::OneAPI),
            "metal" => Some(Self::Metal),
            "directml" | "dml" => Some(Self::Directml),
            "vulkan" => Some(Self::Vulkan),
            "npu" => Some(Self::Npu),
            "cpu" => Some(Self::Cpu),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendInfo {
    pub backend: ComputeBackend,
    pub device_name: String,
    pub vram_gb: Option<f32>,
    pub compute_capability: String,
    pub driver_version: String,
    pub available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendStatus {
    pub backend: ComputeBackend,
    pub device_name: String,
    pub vram_gb: Option<f32>,
    pub status: String,           // "active", "ready", "unavailable"
    pub health: String,           // "healthy", "degraded", "error"
    pub utilization: Option<f32>, // percentage 0-100
    pub temperature: Option<f32>, // celsius
}

#[derive(Debug)]
pub struct BackendRegistry {
    backends: Arc<Mutex<Vec<BackendInfo>>>,
    current: Arc<Mutex<ComputeBackend>>,
}

#[allow(dead_code)]
impl BackendRegistry {
    /// Create a new backend registry and discover available backends using SystemProfile.
    pub fn discover() -> Self {
        // Use the unified SystemProfile for detection
        let profile = ghostlink_core::system_profile::SystemProfile::detect_fast();
        let mut backends = Vec::new();
        let mut current = ComputeBackend::Cpu;

        for gpu in &profile.gpus {
            // Mirrors GpuBackend one-to-one — previously Directml and Vulkan
            // were both collapsed onto OneAPI (a real, distinct technology,
            // unrelated to either) and Npu was dropped entirely, so an
            // NPU-equipped host (e.g. AMD Ryzen AI / Intel Core Ultra) never
            // saw its NPU listed as a selectable backend at all, and a
            // DirectML-only GPU (the common case on Windows without
            // CUDA/ROCm) was mislabeled as "oneapi".
            let backend = match gpu.backend {
                ghostlink_core::host::GpuBackend::Cuda => Some(ComputeBackend::Cuda),
                ghostlink_core::host::GpuBackend::Rocm => Some(ComputeBackend::Rocm),
                ghostlink_core::host::GpuBackend::Metal => Some(ComputeBackend::Metal),
                ghostlink_core::host::GpuBackend::Directml => Some(ComputeBackend::Directml),
                ghostlink_core::host::GpuBackend::Vulkan => Some(ComputeBackend::Vulkan),
                ghostlink_core::host::GpuBackend::Npu => Some(ComputeBackend::Npu),
                ghostlink_core::host::GpuBackend::Cpu => None,
            };

            // Dedup against `b` (this GPU's backend), not `current` (which
            // only ever reflected the first backend seen) — the old check
            // silently dropped every subsequent distinct backend type on any
            // host with more than one kind of accelerator.
            if let Some(b) = backend {
                if !backends.iter().any(|bi: &BackendInfo| bi.backend == b) {
                    backends.push(BackendInfo {
                        backend: b,
                        device_name: gpu.name.clone(),
                        vram_gb: Some(gpu.vram_gb),
                        compute_capability: gpu.compute_capability.clone(),
                        driver_version: gpu.driver_version.clone(),
                        available: true,
                    });
                }
            }
        }
        if let Some(first) = backends.first() {
            current = first.backend;
        }

        // CPU is always available as fallback
        if !backends.iter().any(|b| b.backend == ComputeBackend::Cpu) {
            backends.push(BackendInfo {
                backend: ComputeBackend::Cpu,
                device_name: profile.cpu.brand.clone(),
                vram_gb: None,
                compute_capability: "generic".to_string(),
                driver_version: "native".to_string(),
                available: true,
            });
        }

        Self {
            backends: Arc::new(Mutex::new(backends)),
            current: Arc::new(Mutex::new(current)),
        }
    }

    fn detect_cuda() -> Option<BackendInfo> {
        let output = Command::new("nvidia-smi")
            .args(["--query-gpu=name,memory.total,compute_cap,driver_version"])
            .args(["--format=csv,noheader,nounits"])
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }

        let output_str = String::from_utf8_lossy(&output.stdout);
        let parts: Vec<&str> = output_str.trim().split(',').map(|s| s.trim()).collect();

        if parts.len() >= 4 {
            let device_name = parts[0].to_string();
            let vram_mb = parts[1].parse::<f32>().ok()?;
            let vram_gb = vram_mb / 1024.0;
            let compute_cap = parts[2].to_string();
            let driver_version = parts[3].to_string();

            return Some(BackendInfo {
                backend: ComputeBackend::Cuda,
                device_name,
                vram_gb: Some(vram_gb),
                compute_capability: compute_cap,
                driver_version,
                available: true,
            });
        }

        None
    }

    fn detect_rocm() -> Option<BackendInfo> {
        // On Windows, rocm-smi may not be in PATH; check if ROCm is installed
        #[cfg(target_os = "windows")]
        {
            // Try to detect ROCm via environment variables
            let rocm_installed = std::env::var("ROCM_HOME").is_ok()
                || std::env::var("HIP_PATH").is_ok()
                || std::env::var("HSA_OVERRIDE_GFX_VERSION").is_ok()
                || std::env::var("HIP_VISIBLE_DEVICES").is_ok();

            if rocm_installed {
                // ROCm installation detected via environment
                return Some(BackendInfo {
                    backend: ComputeBackend::Rocm,
                    device_name: "AMD Radeon 860M".to_string(),
                    vram_gb: Some(14.2),
                    compute_capability: std::env::var("HSA_OVERRIDE_GFX_VERSION")
                        .unwrap_or_else(|_| "gfx906".to_string()),
                    driver_version: "ROCm 6.1+".to_string(),
                    available: true,
                });
            }
        }

        let output = Command::new("rocm-smi")
            .args(["--showproductname"])
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }

        let output_str = String::from_utf8_lossy(&output.stdout);
        let device_name = output_str
            .lines()
            .find_map(|line| line.split(':').nth(1).map(|s| s.trim().to_string()))
            .unwrap_or_else(|| "AMD ROCm GPU".to_string());

        // Get VRAM info
        let vram_output = Command::new("rocm-smi")
            .args(["--showmeminfo", "vram"])
            .output()
            .ok();

        let vram_gb = vram_output.and_then(|output| {
            let text = String::from_utf8_lossy(&output.stdout);
            text.lines().find_map(|line| {
                if line.contains("vram Heap") {
                    line.split_whitespace()
                        .find_map(|s| s.parse::<f32>().ok())
                        .map(|mb| mb / 1024.0)
                } else {
                    None
                }
            })
        });

        // Detect GFX version
        let gfx_output = Command::new("rocm-smi").args(["--showid"]).output().ok();

        let compute_capability = gfx_output
            .and_then(|output| {
                let text = String::from_utf8_lossy(&output.stdout);
                text.lines().find_map(|line| {
                    if line.contains("gfx") {
                        line.split_whitespace().find_map(|s| {
                            if s.starts_with("gfx") {
                                Some(s.to_string())
                            } else {
                                None
                            }
                        })
                    } else {
                        None
                    }
                })
            })
            .unwrap_or_else(|| "gfx906".to_string()); // Default to gfx906 (AMD Radeon 860M)

        // Get ROCm version
        let driver_version = Command::new("rocm-smi")
            .args(["--version"])
            .output()
            .ok()
            .and_then(|output| {
                let text = String::from_utf8_lossy(&output.stderr);
                text.lines()
                    .find_map(|line| line.split_whitespace().last().map(|s| s.to_string()))
            })
            .unwrap_or_else(|| "unknown".to_string());

        Some(BackendInfo {
            backend: ComputeBackend::Rocm,
            device_name,
            vram_gb,
            compute_capability,
            driver_version,
            available: true,
        })
    }

    fn detect_oneapi() -> Option<BackendInfo> {
        let output = Command::new("sycl-ls").output().ok()?;

        if !output.status.success() {
            return None;
        }

        let output_str = String::from_utf8_lossy(&output.stdout);

        // Find Intel GPU device
        if let Some(device_line) = output_str.lines().find(|line| line.contains("Intel")) {
            return Some(BackendInfo {
                backend: ComputeBackend::OneAPI,
                device_name: device_line.to_string(),
                vram_gb: None, // oneAPI typically uses shared memory
                compute_capability: "intel_gpu".to_string(),
                driver_version: "oneapi".to_string(),
                available: true,
            });
        }

        None
    }

    #[cfg(target_os = "macos")]
    fn detect_metal() -> Option<BackendInfo> {
        let output = Command::new("system_profiler")
            .args(["SPDisplaysDataType"])
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }

        let output_str = String::from_utf8_lossy(&output.stdout);

        if let Some(chipset_line) = output_str
            .lines()
            .find(|line| line.contains("Chipset Model"))
        {
            let device_name = chipset_line
                .split(':')
                .nth(1)
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|| "Apple GPU".to_string());

            return Some(BackendInfo {
                backend: ComputeBackend::Metal,
                device_name,
                vram_gb: None, // Shared unified memory
                compute_capability: "apple_metal".to_string(),
                driver_version: "metal".to_string(),
                available: true,
            });
        }

        None
    }

    fn detect_cpu_name() -> String {
        #[cfg(target_os = "linux")]
        {
            let output = Command::new("lscpu").output().ok();

            if let Some(output) = output {
                let text = String::from_utf8_lossy(&output.stdout);
                if let Some(line) = text.lines().find(|l| l.contains("Model name")) {
                    if let Some(name) = line.split(':').nth(1) {
                        return name.trim().to_string();
                    }
                }
            }
            "CPU".to_string()
        }

        #[cfg(target_os = "windows")]
        {
            let output = Command::new("wmic")
                .args(["cpu", "get", "name"])
                .output()
                .ok();

            if let Some(output) = output {
                let text = String::from_utf8_lossy(&output.stdout);
                if let Some(name) = text.lines().nth(1) {
                    return name.trim().to_string();
                }
            }
            "CPU".to_string()
        }

        #[cfg(target_os = "macos")]
        {
            let output = Command::new("sysctl")
                .args(["-n", "machdep.cpu.brand_string"])
                .output()
                .ok();

            if let Some(output) = output {
                let name = String::from_utf8_lossy(&output.stdout);
                return name.trim().to_string();
            }
            "CPU".to_string()
        }

        #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
        "CPU".to_string()
    }

    /// Get all available backends
    pub fn available_backends(&self) -> Vec<BackendInfo> {
        self.backends
            .lock()
            .unwrap()
            .iter()
            .filter(|b| b.available)
            .cloned()
            .collect()
    }

    /// Get current active backend
    pub fn current_backend(&self) -> ComputeBackend {
        *self.current.lock().unwrap()
    }

    /// Get a specific backend info
    pub fn get_backend(&self, backend: &ComputeBackend) -> Option<BackendInfo> {
        self.backends
            .lock()
            .unwrap()
            .iter()
            .find(|b| b.backend == *backend)
            .cloned()
    }

    /// Switch to a different backend
    pub fn switch_backend(&self, backend: ComputeBackend) -> Result<(), String> {
        let backends = self.backends.lock().unwrap();

        if !backends.iter().any(|b| b.backend == backend && b.available) {
            return Err(format!("Backend {} is not available", backend.as_str()));
        }

        drop(backends);
        *self.current.lock().unwrap() = backend;
        Ok(())
    }

    /// Get backend status
    pub fn get_status(&self, backend: &ComputeBackend) -> Option<BackendStatus> {
        let info = self.get_backend(backend)?;
        let current = self.current_backend();
        let is_active = current == *backend;

        let status = if is_active { "active" } else { "ready" }.to_string();
        let health = "healthy".to_string(); // TODO: implement health checks

        Some(BackendStatus {
            backend: info.backend,
            device_name: info.device_name,
            vram_gb: info.vram_gb,
            status,
            health,
            utilization: None, // TODO: query nvidia-smi or rocm-smi
            temperature: None, // TODO: query nvidia-smi or rocm-smi
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backend_enum() {
        assert_eq!(ComputeBackend::Rocm.as_str(), "rocm");
        assert_eq!(ComputeBackend::Cuda.as_str(), "cuda");
        assert_eq!(ComputeBackend::from_str("rocm"), Some(ComputeBackend::Rocm));
    }

    #[test]
    fn test_backend_enum_covers_directml_vulkan_npu() {
        // Regression: these three used to have no ComputeBackend representation
        // at all (Directml/Vulkan were both mapped onto the unrelated OneAPI
        // variant in discover(), Npu was silently dropped), so a DirectML GPU
        // or an NPU could never actually be selected via the API.
        for (variant, s) in [
            (ComputeBackend::Directml, "directml"),
            (ComputeBackend::Vulkan, "vulkan"),
            (ComputeBackend::Npu, "npu"),
        ] {
            assert_eq!(variant.as_str(), s);
            assert_eq!(ComputeBackend::from_str(s), Some(variant));
        }
        assert_eq!(
            ComputeBackend::from_str("dml"),
            Some(ComputeBackend::Directml)
        );
    }

    #[test]
    fn test_backend_registry_discover() {
        let registry = BackendRegistry::discover();
        let backends = registry.available_backends();

        // CPU should always be available
        assert!(backends.iter().any(|b| b.backend == ComputeBackend::Cpu));
    }

    #[test]
    fn test_backend_registry_current() {
        let registry = BackendRegistry::discover();
        let current = registry.current_backend();

        // Should have a current backend
        let backends = registry.available_backends();
        assert!(backends.iter().any(|b| b.backend == current));
    }

    #[test]
    fn test_backend_switch() {
        let registry = BackendRegistry::discover();

        // Switch to CPU should always work
        assert!(registry.switch_backend(ComputeBackend::Cpu).is_ok());
        assert_eq!(registry.current_backend(), ComputeBackend::Cpu);
    }

    #[test]
    fn test_backend_info_get() {
        let registry = BackendRegistry::discover();
        let cpu_info = registry.get_backend(&ComputeBackend::Cpu);

        assert!(cpu_info.is_some());
        assert_eq!(cpu_info.unwrap().backend, ComputeBackend::Cpu);
    }

    #[test]
    fn test_backend_status() {
        let registry = BackendRegistry::discover();
        let current = registry.current_backend();
        let status = registry.get_status(&current);

        assert!(status.is_some());
        let s = status.unwrap();
        assert_eq!(s.status, "active");
        assert_eq!(s.health, "healthy");
    }
}
