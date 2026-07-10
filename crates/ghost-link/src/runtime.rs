//! Runtime detection and model availability management
//! Supports GPU (CUDA/Metal), NPU, and CPU runtimes with auto-detection

use serde::{Deserialize, Serialize};

#[allow(clippy::upper_case_acronyms)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Runtime {
    CUDA,  // NVIDIA GPUs
    Metal, // Apple Silicon / Apple GPUs
    ROCm,  // AMD GPUs
    NPU,   // Neural Processing Units (Qualcomm, MediaTek)
    CPU,   // CPU fallback
}

impl std::fmt::Display for Runtime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Runtime::CUDA => write!(f, "CUDA (NVIDIA GPU)"),
            Runtime::Metal => write!(f, "Metal (Apple Silicon)"),
            Runtime::ROCm => write!(f, "ROCm (AMD GPU)"),
            Runtime::NPU => write!(f, "NPU (Neural Processor)"),
            Runtime::CPU => write!(f, "CPU (Default)"),
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
        vec![
            Self::detect_cuda(),
            Self::detect_metal(),
            Self::detect_rocm(),
            Self::detect_npu(),
            Self::detect_cpu(),
        ]
        .into_iter()
        .flatten()
        .collect()
    }

    /// Detect primary runtime (best available)
    pub fn detect_primary() -> Runtime {
        let runtimes = Self::detect();
        runtimes
            .first()
            .map(|r| r.detected_runtime)
            .unwrap_or(Runtime::CPU)
    }

    /// Detect NVIDIA CUDA
    fn detect_cuda() -> Option<RuntimeInfo> {
        // Check for CUDA toolkit
        if std::path::Path::new("/usr/local/cuda").exists()
            || std::env::var("CUDA_PATH").is_ok()
            || std::env::var("CUDA_HOME").is_ok()
        {
            return Some(RuntimeInfo {
                detected_runtime: Runtime::CUDA,
                is_available: true,
                compute_capability: Some("7.0+".to_string()),
                memory_gb: Self::detect_gpu_memory(),
                device_count: Some(1),
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

    /// Detect AMD ROCm
    fn detect_rocm() -> Option<RuntimeInfo> {
        #[cfg(feature = "rocm")]
        {
            if std::path::Path::new("/opt/rocm").exists() || std::env::var("ROCM_HOME").is_ok() {
                return Some(RuntimeInfo {
                    detected_runtime: Runtime::ROCm,
                    is_available: true,
                    compute_capability: Some("RDNA/CDNA".to_string()),
                    memory_gb: Self::detect_gpu_memory(),
                    device_count: Some(1),
                });
            }
        }
        None
    }

    /// Detect Neural Processing Units
    fn detect_npu() -> Option<RuntimeInfo> {
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
        // Try to detect GPU memory (simplified)
        // In production, use nvidia-smi, rocm-smi, etc.
        #[cfg(any(feature = "cuda", feature = "rocm"))]
        {
            Some(8.0) // Default assumption: 8GB
        }
        #[cfg(not(any(feature = "cuda", feature = "rocm")))]
        {
            None
        }
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
                recommended_runtimes: vec![Runtime::CPU, Runtime::NPU],
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
                recommended_runtimes: vec![Runtime::CPU, Runtime::NPU],
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
                recommended_runtimes: vec![Runtime::CPU, Runtime::CUDA, Runtime::Metal],
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
                recommended_runtimes: vec![Runtime::CPU, Runtime::CUDA, Runtime::Metal],
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
                recommended_runtimes: vec![Runtime::CUDA, Runtime::Metal, Runtime::ROCm],
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
                recommended_runtimes: vec![Runtime::CUDA, Runtime::Metal, Runtime::ROCm],
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
                recommended_runtimes: vec![Runtime::CUDA, Runtime::ROCm],
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
                recommended_runtimes: vec![Runtime::CUDA, Runtime::Metal],
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
                recommended_runtimes: vec![Runtime::CUDA, Runtime::ROCm],
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

    #[test]
    fn test_runtime_detection() {
        let runtimes = RuntimeDetector::detect();
        assert!(!runtimes.is_empty(), "CPU should always be detected");
    }

    #[test]
    fn test_cpu_always_available() {
        let primary = RuntimeDetector::detect_primary();
        // At worst, CPU should be available
        assert_eq!(primary, Runtime::CPU);
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
}
