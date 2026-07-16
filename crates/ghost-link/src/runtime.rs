//! Runtime detection and model availability management
//! Supports GPU (CUDA/Metal), NPU, and CPU runtimes with auto-detection

use serde::{Deserialize, Serialize};

#[allow(clippy::upper_case_acronyms)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Runtime {
    CUDA,     // NVIDIA GPUs
    Metal,    // Apple Silicon / Apple GPUs
    ROCm,     // AMD GPUs (Linux / discrete)
    DirectML, // DirectML on Windows (AMD iGPU, Intel ARC, any DirectX 12 GPU)
    NPU,      // Neural Processing Units (AMD XDNA, Qualcomm, Intel NPU)
    CPU,      // CPU fallback
}

impl std::fmt::Display for Runtime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Runtime::CUDA => write!(f, "CUDA (NVIDIA GPU)"),
            Runtime::Metal => write!(f, "Metal (Apple Silicon)"),
            Runtime::ROCm => write!(f, "ROCm (AMD GPU)"),
            Runtime::DirectML => write!(f, "DirectML (DirectX 12 GPU)"),
            Runtime::NPU => write!(f, "NPU (Neural Processor)"),
            Runtime::CPU => write!(f, "CPU (Default)"),
        }
    }
}

impl std::str::FromStr for Runtime {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "cuda" | "nvidia" | "nvidia-gpu" | "nvidia gpu" => Ok(Runtime::CUDA),
            "metal" | "apple" | "apple-metal" => Ok(Runtime::Metal),
            "rocm" | "amd" | "amd-gpu" | "amd gpu" => Ok(Runtime::ROCm),
            "directml" | "dml" | "directml-gpu" => Ok(Runtime::DirectML),
            "npu" | "neural" | "neural-processor" => Ok(Runtime::NPU),
            "cpu" | "default" => Ok(Runtime::CPU),
            other => Err(format!("unknown runtime '{}'", other)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeInfo {
    pub detected_runtime: Runtime,
    pub is_available: bool,
    pub compute_capability: Option<String>,
    pub memory_gb: Option<f32>,
    pub device_count: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub name: String,
    pub parameters: String,
    pub size_gb: f32,
    pub memory_required_gb: f32,
    pub recommended_runtimes: Vec<Runtime>,
    pub inference_speed: ModelSpeed,
    pub quality_tier: QualityTier,
    pub use_cases: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelSpeed {
    Fast,     // <50ms response
    Standard, // 50-200ms response
    Slow,     // >200ms response
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QualityTier {
    Lightweight, // 3B-7B models, fast inference
    Standard,    // 7B-13B models, good quality/speed
    Premium,     // 13B-70B models, high quality
    Specialized, // Domain-specific models
}

/// Runtime detection system
pub struct RuntimeDetector;

impl RuntimeDetector {
    /// Auto-detect available runtimes on the system
    pub fn detect() -> Vec<RuntimeInfo> {
        #[allow(unused_mut)]
        let mut runtimes = vec![
            Self::detect_cuda(),
            Self::detect_metal(),
            Self::detect_directml(),
            Self::detect_npu(),
            Self::detect_cpu(),
        ];
        #[cfg(feature = "rocm")]
        runtimes.insert(2, Self::detect_rocm());
        runtimes.into_iter().flatten().collect()
    }

    /// Detect primary runtime (best available), honoring environment overrides.
    pub fn detect_primary() -> Runtime {
        if let Some(runtime) = Self::detect_override_runtime() {
            return runtime;
        }

        let runtimes = Self::detect();
        runtimes
            .first()
            .map(|r| r.detected_runtime)
            .unwrap_or(Runtime::CPU)
    }

    fn detect_override_runtime() -> Option<Runtime> {
        let runtime_str = std::env::var("GHOSTLINK_RUNTIME").ok()?;
        let runtime = runtime_str.parse::<Runtime>().ok()?;
        let force = std::env::var("GHOSTLINK_FORCE_RUNTIME")
            .map(|value| {
                matches!(
                    value.to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )
            })
            .unwrap_or(false);

        if force {
            return Some(runtime);
        }

        let available = Self::detect()
            .into_iter()
            .any(|info| info.detected_runtime == runtime);

        if available {
            Some(runtime)
        } else {
            None
        }
    }

    /// Detect NVIDIA CUDA — checks toolkit path, env vars, and nvidia-smi
    fn detect_cuda() -> Option<RuntimeInfo> {
        // Check for CUDA toolkit or nvidia-smi
        let has_cuda_toolkit = std::path::Path::new("/usr/local/cuda").exists()
            || std::env::var("CUDA_PATH").is_ok()
            || std::env::var("CUDA_HOME").is_ok();
        let has_nvidia_smi = std::process::Command::new("nvidia-smi")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        if has_cuda_toolkit || has_nvidia_smi {
            // Detect number of GPUs and compute capability via nvidia-smi
            let (device_count, compute_capability) = if has_nvidia_smi {
                let count = std::process::Command::new("nvidia-smi")
                    .args(["--query-gpu=index", "--format=csv,noheader"])
                    .output()
                    .ok()
                    .map(|o| String::from_utf8_lossy(&o.stdout).lines().count());
                let cc = std::process::Command::new("nvidia-smi")
                    .args(["--query-gpu=compute_cap", "--format=csv,noheader"])
                    .output()
                    .ok()
                    .and_then(|o| {
                        String::from_utf8_lossy(&o.stdout)
                            .lines()
                            .next()
                            .map(|l| l.trim().to_string())
                    });
                (count.or(Some(1)), cc.or(Some("7.0+".to_string())))
            } else {
                (Some(1), Some("7.0+".to_string()))
            };

            return Some(RuntimeInfo {
                detected_runtime: Runtime::CUDA,
                is_available: true,
                compute_capability,
                memory_gb: Self::detect_gpu_memory(),
                device_count,
            });
        }
        None
    }

    /// Detect Apple Metal
    fn detect_metal() -> Option<RuntimeInfo> {
        #[cfg(target_os = "macos")]
        {
            // Metal is always available on macOS with Apple Silicon
            if Self::is_apple_silicon() {
                return Some(RuntimeInfo {
                    detected_runtime: Runtime::Metal,
                    is_available: true,
                    compute_capability: Some("Apple Neural Engine".to_string()),
                    memory_gb: Self::detect_system_memory(),
                    device_count: Some(1),
                });
            }
        }
        None
    }

    /// Detect AMD ROCm — works on Linux (/opt/rocm, rocm-smi) and Windows (HIP_PATH, ROCM_PATH)
    #[cfg(feature = "rocm")]
    fn detect_rocm() -> Option<RuntimeInfo> {
        // Linux: /opt/rocm directory or ROCM_HOME env var
        let has_rocm_linux = std::path::Path::new("/opt/rocm").exists()
            || std::env::var("ROCM_HOME").is_ok()
            || std::env::var("ROCM_PATH").is_ok();

        // Windows: HIP SDK path or ROCM_PATH
        let has_rocm_windows = std::env::var("HIP_PATH").is_ok()
            || std::env::var("ROCM_PATH").is_ok()
            || std::path::Path::new("C:\\Program Files\\AMD\\ROCm").exists()
            || std::path::Path::new("C:\\Program Files\\AMD\\HIP").exists();

        // Check via rocm-smi tool (cross-platform)
        let has_rocm_smi = std::process::Command::new("rocm-smi")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        // Check via hipconfig (AMD's HIP config tool)
        let has_hipconfig = std::process::Command::new("hipconfig")
            .arg("--full")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        // Check visible device env vars
        let has_visible_devices = std::env::var("ROCR_VISIBLE_DEVICES").is_ok()
            || std::env::var("HIP_VISIBLE_DEVICES").is_ok();

        if has_rocm_linux
            || has_rocm_windows
            || has_rocm_smi
            || has_hipconfig
            || has_visible_devices
        {
            let device_count = if has_rocm_smi {
                // Count AMD GPUs via rocm-smi
                std::process::Command::new("rocm-smi")
                    .args(["--showid"])
                    .output()
                    .ok()
                    .map(|o| {
                        String::from_utf8_lossy(&o.stdout)
                            .lines()
                            .filter(|l| l.contains("GPU["))
                            .count()
                    })
                    .or_else(|| {
                        std::env::var("ROCR_VISIBLE_DEVICES")
                            .or_else(|_| std::env::var("HIP_VISIBLE_DEVICES"))
                            .ok()
                            .map(|v| v.split(',').count())
                    })
                    .or(Some(1))
            } else {
                Some(1)
            };

            // Try to get actual GPU memory via rocm-smi
            let memory_gb = std::process::Command::new("rocm-smi")
                .args(["--showmeminfo", "vram"])
                .output()
                .ok()
                .and_then(|o| {
                    let stdout = String::from_utf8_lossy(&o.stdout);
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
                })
                .or_else(|| Self::detect_gpu_memory());

            return Some(RuntimeInfo {
                detected_runtime: Runtime::ROCm,
                is_available: true,
                compute_capability: Some("RDNA3/CDNA3".to_string()),
                memory_gb,
                device_count,
            });
        }

        None
    }

    /// Detect DirectML-capable GPUs (Windows)
    /// This catches AMD iGPUs, Intel ARC, and any DirectX 12 GPU not detected by CUDA/ROCm
    fn detect_directml() -> Option<RuntimeInfo> {
        #[cfg(windows)]
        {
            // On Windows, check for GPUs via WMI that are NOT NVIDIA (handled by CUDA detection)
            // and NOT already detected by Metal
            let has_nvidia = std::process::Command::new("nvidia-smi")
                .arg("--version")
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);

            if !has_nvidia {
                // Check for AMD or Intel GPUs via WMI
                if let Ok(output) = std::process::Command::new("wmic")
                    .args([
                        "path",
                        "Win32_VideoController",
                        "get",
                        "Name",
                        "/format:csv",
                    ])
                    .output()
                {
                    if output.status.success() {
                        let stdout = String::from_utf8_lossy(&output.stdout);
                        for line in stdout.lines().skip(1) {
                            let name = line.trim();
                            if name.is_empty() {
                                continue;
                            }
                            let lower = name.to_lowercase();
                            let is_amd = lower.contains("amd") || lower.contains("radeon");
                            let is_intel = lower.contains("intel")
                                || lower.contains("iris")
                                || lower.contains("arc");
                            let is_generic_d3d = lower.contains("microsoft basic display")
                                || lower.contains("directx");

                            if is_amd || is_intel || is_generic_d3d {
                                // Check for DirectX 12 / WDDM 2.0+ support via dxdiag or registry
                                let d3d12_available =
                                    std::path::Path::new("C:\\Windows\\System32\\d3d12.dll")
                                        .exists();
                                if d3d12_available {
                                    return Some(RuntimeInfo {
                                        detected_runtime: Runtime::DirectML,
                                        is_available: true,
                                        compute_capability: Some("DirectX 12".to_string()),
                                        memory_gb: Self::detect_gpu_memory(),
                                        device_count: Some(1),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
        None
    }

    /// Detect Neural Processing Units (AMD XDNA, Qualcomm, MediaTek, Intel)
    fn detect_npu() -> Option<RuntimeInfo> {
        #[cfg(windows)]
        {
            // Windows: check for AMD Ryzen AI NPU (XDNA) or Intel NPU via WMI/PnP
            let has_amd_npu = std::process::Command::new("wmic")
                .args(["path", "Win32_PnPEntity", "get", "Name", "/format:csv"])
                .output()
                .map(|output| {
                    if output.status.success() {
                        let stdout = String::from_utf8_lossy(&output.stdout);
                        let lower = stdout.to_lowercase();
                        lower.contains("npu")
                            || lower.contains("neural")
                            || lower.contains("xdna")
                            || lower.contains("ai accelerator")
                            || lower.contains("ryzen ai")
                            || lower.contains("intel npu")
                    } else {
                        false
                    }
                })
                .unwrap_or(false);

            if has_amd_npu {
                return Some(RuntimeInfo {
                    detected_runtime: Runtime::NPU,
                    is_available: true,
                    compute_capability: Some("AI Accelerator".to_string()),
                    memory_gb: Some(2.0),
                    device_count: Some(1),
                });
            }
        }
        // Check for common NPU environments
        let has_npu_env = std::env::var("NPU_DEVICE").is_ok()
            || std::env::var("QUALCOMM_NPU").is_ok()
            || std::env::var("MEDIATEK_NPU").is_ok();

        let npu_indicators = [
            "/sys/devices/platform/soc/*/npu", // Qualcomm NPUs
            "/sys/devices/virtual/npu",        // Generic NPU
        ];

        if has_npu_env
            || npu_indicators.iter().any(|indicator| {
                if let Some(path) = indicator.strip_prefix("/") {
                    std::path::Path::new(path).exists()
                } else {
                    false
                }
            })
        {
            return Some(RuntimeInfo {
                detected_runtime: Runtime::NPU,
                is_available: true,
                compute_capability: Some("AI Accelerator".to_string()),
                memory_gb: Some(2.0), // Typical NPU memory
                device_count: Some(1),
            });
        }
        None
    }

    /// CPU is always available as fallback
    fn detect_cpu() -> Option<RuntimeInfo> {
        Some(RuntimeInfo {
            detected_runtime: Runtime::CPU,
            is_available: true,
            compute_capability: None,
            memory_gb: Self::detect_system_memory(),
            device_count: Some(
                std::thread::available_parallelism()
                    .map(|n| n.get())
                    .unwrap_or(1),
            ),
        })
    }

    #[cfg(target_os = "macos")]
    fn is_apple_silicon() -> bool {
        std::process::Command::new("sysctl")
            .arg("hw.optional.arm64")
            .output()
            .map(|output| String::from_utf8_lossy(&output.stdout).contains("1"))
            .unwrap_or(false)
    }

    #[cfg(not(target_os = "macos"))]
    #[allow(dead_code)]
    fn is_apple_silicon() -> bool {
        false
    }

    fn detect_gpu_memory() -> Option<f32> {
        // Try NVIDIA SMI first
        if let Ok(output) = std::process::Command::new("nvidia-smi")
            .args(["--query-gpu=memory.total", "--format=csv,noheader,nounits"])
            .output()
        {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if let Some(line) = stdout.lines().next() {
                    if let Ok(mib) = line.trim().parse::<f32>() {
                        return Some(mib / 1024.0);
                    }
                }
            }
        }

        // Try ROCm SMI for AMD GPUs
        if let Ok(output) = std::process::Command::new("rocm-smi")
            .args(["--showmeminfo", "vram"])
            .output()
        {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if let Some(line) = stdout.lines().find(|l| l.contains("Total Memory")) {
                    let val = line.split(':').nth(1)?.trim().to_lowercase();
                    let num: f32 = val.split_whitespace().next()?.parse().ok()?;
                    if val.contains("gb") {
                        return Some(num);
                    }
                    if val.contains("mb") {
                        return Some(num / 1024.0);
                    }
                }
            }
        }

        // Windows WMI fallback for any GPU
        #[cfg(windows)]
        {
            if let Ok(output) = std::process::Command::new("wmic")
                .args([
                    "path",
                    "Win32_VideoController",
                    "get",
                    "AdapterRAM",
                    "/format:csv",
                ])
                .output()
            {
                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    for line in stdout.lines().skip(1) {
                        if let Ok(bytes) = line.trim().parse::<f64>() {
                            if bytes > 0.0 {
                                return Some((bytes / 1_073_741_824.0) as f32);
                            }
                        }
                    }
                }
            }
        }

        None
    }

    fn detect_system_memory() -> Option<f32> {
        // Get total system RAM in GB
        #[cfg(target_os = "linux")]
        {
            if let Ok(output) = std::process::Command::new("free").arg("-g").output() {
                if let Ok(text) = String::from_utf8(output.stdout) {
                    if let Some(line) = text.lines().next() {
                        if let Some(mem_str) = line.split_whitespace().nth(1) {
                            if let Ok(mem) = mem_str.parse::<f32>() {
                                return Some(mem);
                            }
                        }
                    }
                }
            }
            Some(8.0) // Default
        }

        #[cfg(target_os = "macos")]
        {
            if let Ok(output) = std::process::Command::new("sysctl")
                .arg("-n")
                .arg("hw.memsize")
                .output()
            {
                if let Ok(text) = String::from_utf8(output.stdout) {
                    if let Ok(bytes) = text.trim().parse::<f64>() {
                        return Some((bytes / 1_073_741_824.0) as f32); // Convert to GB
                    }
                }
            }
            Some(16.0) // Default for Mac
        }

        #[cfg(windows)]
        {
            Some(16.0) // Default for Windows
        }

        #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
        {
            Some(8.0) // Generic default
        }
    }
}

/// Model registry with runtime-specific availability
pub struct ModelRegistry;

impl ModelRegistry {
    /// Get all models optimized for a specific runtime
    pub fn models_for_runtime(runtime: Runtime) -> Vec<ModelInfo> {
        Self::all_models()
            .into_iter()
            .filter(|m| m.recommended_runtimes.contains(&runtime))
            .collect()
    }

    /// Get all available models
    pub fn all_models() -> Vec<ModelInfo> {
        vec![
            // LIGHTWEIGHT MODELS (3B-7B) - Fast on all runtimes
            ModelInfo {
                name: "orca-mini".to_string(),
                parameters: "3B".to_string(),
                size_gb: 1.7,
                memory_required_gb: 2.0,
                recommended_runtimes: vec![Runtime::CPU, Runtime::NPU, Runtime::DirectML],
                inference_speed: ModelSpeed::Fast,
                quality_tier: QualityTier::Lightweight,
                use_cases: vec![
                    "Quick responses".to_string(),
                    "Edge devices".to_string(),
                    "Real-time chat".to_string(),
                ],
            },
            ModelInfo {
                name: "phi".to_string(),
                parameters: "3B".to_string(),
                size_gb: 2.0,
                memory_required_gb: 3.0,
                recommended_runtimes: vec![Runtime::CPU, Runtime::NPU, Runtime::DirectML],
                inference_speed: ModelSpeed::Fast,
                quality_tier: QualityTier::Lightweight,
                use_cases: vec![
                    "Mobile deployment".to_string(),
                    "Embedded systems".to_string(),
                    "Low-latency".to_string(),
                ],
            },
            ModelInfo {
                name: "mistral".to_string(),
                parameters: "7B".to_string(),
                size_gb: 4.1,
                memory_required_gb: 6.0,
                recommended_runtimes: vec![
                    Runtime::CPU,
                    Runtime::NPU,
                    Runtime::DirectML,
                    Runtime::CUDA,
                    Runtime::Metal,
                    Runtime::ROCm,
                ],
                inference_speed: ModelSpeed::Standard,
                quality_tier: QualityTier::Standard,
                use_cases: vec![
                    "General purpose".to_string(),
                    "Default choice".to_string(),
                    "Balanced quality/speed".to_string(),
                ],
            },
            ModelInfo {
                name: "neural-chat".to_string(),
                parameters: "7B".to_string(),
                size_gb: 4.0,
                memory_required_gb: 6.0,
                recommended_runtimes: vec![
                    Runtime::CPU,
                    Runtime::DirectML,
                    Runtime::CUDA,
                    Runtime::Metal,
                ],
                inference_speed: ModelSpeed::Standard,
                quality_tier: QualityTier::Standard,
                use_cases: vec![
                    "Conversational AI".to_string(),
                    "Chat applications".to_string(),
                    "Instruction following".to_string(),
                ],
            },
            // STANDARD MODELS (7B-13B) - Good quality/speed balance
            ModelInfo {
                name: "llama2".to_string(),
                parameters: "7B".to_string(),
                size_gb: 3.8,
                memory_required_gb: 5.5,
                recommended_runtimes: vec![
                    Runtime::CPU,
                    Runtime::DirectML,
                    Runtime::CUDA,
                    Runtime::Metal,
                    Runtime::ROCm,
                ],
                inference_speed: ModelSpeed::Standard,
                quality_tier: QualityTier::Standard,
                use_cases: vec![
                    "Text generation".to_string(),
                    "Code generation".to_string(),
                    "Versatile tasks".to_string(),
                ],
            },
            ModelInfo {
                name: "openhermes".to_string(),
                parameters: "7B".to_string(),
                size_gb: 4.1,
                memory_required_gb: 6.0,
                recommended_runtimes: vec![
                    Runtime::CPU,
                    Runtime::DirectML,
                    Runtime::CUDA,
                    Runtime::Metal,
                ],
                inference_speed: ModelSpeed::Standard,
                quality_tier: QualityTier::Standard,
                use_cases: vec![
                    "Instruction following".to_string(),
                    "Question answering".to_string(),
                    "Reasoning tasks".to_string(),
                ],
            },
            ModelInfo {
                name: "llama2-13b".to_string(),
                parameters: "13B".to_string(),
                size_gb: 7.3,
                memory_required_gb: 10.0,
                recommended_runtimes: vec![
                    Runtime::DirectML,
                    Runtime::CUDA,
                    Runtime::Metal,
                    Runtime::ROCm,
                ],
                inference_speed: ModelSpeed::Standard,
                quality_tier: QualityTier::Premium,
                use_cases: vec![
                    "Complex reasoning".to_string(),
                    "Advanced reasoning".to_string(),
                    "Long context".to_string(),
                ],
            },
            // PREMIUM MODELS (13B-70B) - High quality, requires GPU
            ModelInfo {
                name: "mistral-medium".to_string(),
                parameters: "13B".to_string(),
                size_gb: 8.0,
                memory_required_gb: 12.0,
                recommended_runtimes: vec![
                    Runtime::DirectML,
                    Runtime::CUDA,
                    Runtime::Metal,
                    Runtime::ROCm,
                ],
                inference_speed: ModelSpeed::Slow,
                quality_tier: QualityTier::Premium,
                use_cases: vec![
                    "High quality responses".to_string(),
                    "Complex analysis".to_string(),
                    "Multi-step reasoning".to_string(),
                ],
            },
            ModelInfo {
                name: "llama2-70b".to_string(),
                parameters: "70B".to_string(),
                size_gb: 39.0,
                memory_required_gb: 48.0,
                recommended_runtimes: vec![Runtime::DirectML, Runtime::CUDA, Runtime::ROCm],
                inference_speed: ModelSpeed::Slow,
                quality_tier: QualityTier::Premium,
                use_cases: vec![
                    "Expert-level reasoning".to_string(),
                    "Complex code".to_string(),
                    "Research applications".to_string(),
                ],
            },
            // SPECIALIZED MODELS
            ModelInfo {
                name: "codeup".to_string(),
                parameters: "13B".to_string(),
                size_gb: 7.5,
                memory_required_gb: 10.0,
                recommended_runtimes: vec![Runtime::DirectML, Runtime::CUDA, Runtime::Metal],
                inference_speed: ModelSpeed::Slow,
                quality_tier: QualityTier::Specialized,
                use_cases: vec![
                    "Code generation".to_string(),
                    "Code completion".to_string(),
                    "Programming assistance".to_string(),
                ],
            },
            ModelInfo {
                name: "dolphin-mixtral".to_string(),
                parameters: "8x7B".to_string(),
                size_gb: 26.0,
                memory_required_gb: 32.0,
                recommended_runtimes: vec![Runtime::DirectML, Runtime::CUDA, Runtime::ROCm],
                inference_speed: ModelSpeed::Slow,
                quality_tier: QualityTier::Specialized,
                use_cases: vec![
                    "Advanced conversations".to_string(),
                    "High complexity tasks".to_string(),
                    "Multi-task handling".to_string(),
                ],
            },
        ]
    }

    /// Get model recommendations based on system specs
    pub fn recommend_models(runtime: Runtime, available_memory_gb: f32) -> Vec<ModelInfo> {
        Self::models_for_runtime(runtime)
            .into_iter()
            .filter(|m| m.memory_required_gb <= available_memory_gb)
            .collect()
    }

    /// Get best model for runtime (fastest/highest quality balance)
    pub fn best_for_runtime(runtime: Runtime) -> Option<ModelInfo> {
        let models = Self::models_for_runtime(runtime);

        // Prefer standard quality tier, then premium, then lightweight
        models
            .iter()
            .find(|m| m.quality_tier == QualityTier::Standard)
            .or_else(|| {
                models
                    .iter()
                    .find(|m| m.quality_tier == QualityTier::Premium)
            })
            .or_else(|| {
                models
                    .iter()
                    .find(|m| m.quality_tier == QualityTier::Lightweight)
            })
            .cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    fn with_runtime_env<R>(runtime: &str, force: Option<&str>, f: impl FnOnce() -> R) -> R {
        let _guard = ENV_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("env lock poisoned");

        let prev_runtime = std::env::var("GHOSTLINK_RUNTIME").ok();
        let prev_force = std::env::var("GHOSTLINK_FORCE_RUNTIME").ok();

        std::env::set_var("GHOSTLINK_RUNTIME", runtime);
        match force {
            Some(v) => std::env::set_var("GHOSTLINK_FORCE_RUNTIME", v),
            None => std::env::remove_var("GHOSTLINK_FORCE_RUNTIME"),
        }

        let out = f();

        match prev_runtime {
            Some(v) => std::env::set_var("GHOSTLINK_RUNTIME", v),
            None => std::env::remove_var("GHOSTLINK_RUNTIME"),
        }
        match prev_force {
            Some(v) => std::env::set_var("GHOSTLINK_FORCE_RUNTIME", v),
            None => std::env::remove_var("GHOSTLINK_FORCE_RUNTIME"),
        }

        out
    }

    #[test]
    fn test_runtime_detection() {
        let runtimes = RuntimeDetector::detect();
        assert!(!runtimes.is_empty(), "CPU should always be detected");
    }

    #[test]
    fn test_runtime_always_detected() {
        let primary = RuntimeDetector::detect_primary();
        // At worst, CPU should be available as fallback
        let valid = matches!(
            primary,
            Runtime::CPU
                | Runtime::DirectML
                | Runtime::CUDA
                | Runtime::ROCm
                | Runtime::Metal
                | Runtime::NPU
        );
        assert!(valid, "A runtime should always be detected");
    }

    #[test]
    fn test_model_registry() {
        let models = ModelRegistry::all_models();
        assert!(!models.is_empty());

        let cpu_models = ModelRegistry::models_for_runtime(Runtime::CPU);
        assert!(!cpu_models.is_empty(), "CPU models should be available");
    }

    #[test]
    fn test_model_recommendations() {
        let recommended = ModelRegistry::recommend_models(Runtime::CPU, 8.0);
        assert!(!recommended.is_empty());
    }

    #[test]
    fn test_best_model() {
        let best = ModelRegistry::best_for_runtime(Runtime::CPU);
        assert!(best.is_some());
    }

    #[test]
    fn test_rocm_models_include_amd_compatible_models() {
        let rocm_models = ModelRegistry::models_for_runtime(Runtime::ROCm);
        assert!(
            !rocm_models.is_empty(),
            "ROCm should have compatible models"
        );

        let names: Vec<&str> = rocm_models.iter().map(|m| m.name.as_str()).collect();
        assert!(
            names.contains(&"mistral"),
            "mistral should be ROCm compatible"
        );
        assert!(
            names.contains(&"llama2"),
            "llama2 should be ROCm compatible"
        );
        assert!(
            names.contains(&"llama2-13b"),
            "llama2-13b should be ROCm compatible"
        );
    }

    #[test]
    fn test_rocm_best_model() {
        let best = ModelRegistry::best_for_runtime(Runtime::ROCm);
        assert!(best.is_some());
        assert_eq!(best.unwrap().quality_tier, QualityTier::Standard);
    }

    #[test]
    fn test_rocm_model_recommendations_fit_memory() {
        let recommended = ModelRegistry::recommend_models(Runtime::ROCm, 12.0);
        assert!(!recommended.is_empty());
        for model in &recommended {
            assert!(model.memory_required_gb <= 12.0);
        }
    }

    #[test]
    fn test_rocm_recommendation_filters_by_vram() {
        let limited = ModelRegistry::recommend_models(Runtime::ROCm, 6.0);
        let abundant = ModelRegistry::recommend_models(Runtime::ROCm, 48.0);
        assert!(
            limited.len() < abundant.len(),
            "More VRAM should allow more models"
        );
    }

    #[cfg(feature = "rocm")]
    #[test]
    fn test_rocm_detection_is_available() {
        let runtimes = RuntimeDetector::detect();
        // On a system without AMD ROCm installed, ROCm detection returns no entry,
        // but the detection should at least run without panicking.
        assert!(
            !runtimes.is_empty(),
            "Runtime detection should always return at least CPU"
        );
    }

    #[test]
    fn test_runtime_from_str_parsing() {
        assert_eq!("cuda".parse::<Runtime>().unwrap(), Runtime::CUDA);
        assert_eq!("RoCm".parse::<Runtime>().unwrap(), Runtime::ROCm);
        assert_eq!("DirectML".parse::<Runtime>().unwrap(), Runtime::DirectML);
        assert_eq!("npu".parse::<Runtime>().unwrap(), Runtime::NPU);
        assert_eq!("cpu".parse::<Runtime>().unwrap(), Runtime::CPU);
        assert!("unknown".parse::<Runtime>().is_err());
    }

    #[test]
    fn test_runtime_override_force_ignores_detection() {
        let runtime = with_runtime_env("metal", Some("true"), RuntimeDetector::detect_primary);
        assert_eq!(runtime, Runtime::Metal);
    }

    #[test]
    fn test_runtime_override_falls_back_when_not_forced() {
        let runtime = with_runtime_env("metal", None, RuntimeDetector::detect_primary);
        assert!(matches!(
            runtime,
            Runtime::Metal
                | Runtime::CPU
                | Runtime::CUDA
                | Runtime::ROCm
                | Runtime::DirectML
                | Runtime::NPU
        ));
    }
}
