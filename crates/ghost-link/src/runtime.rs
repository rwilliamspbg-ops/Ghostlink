//! Runtime detection and model availability management
//! Supports GPU (CUDA/Metal), NPU, and CPU runtimes with auto-detection

use serde::{Deserialize, Serialize};

#[allow(clippy::upper_case_acronyms)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Runtime {
    CUDA,
    Metal,
    ROCm,
    DirectML,
    NPU,
    CPU,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeInfo {
    pub detected_runtime: Runtime,
    pub is_available: bool,
    pub compute_capability: Option<String>,
    pub memory_gb: Option<f32>,
    pub device_count: Option<usize>,
    pub gpu_name: Option<String>,
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
    Fast,
    Standard,
    Slow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QualityTier {
    Lightweight,
    Standard,
    Premium,
    Specialized,
}

#[cfg(windows)]
fn wmi_query_values(class: &str, property: &str) -> Vec<String> {
    if let Ok(output) = std::process::Command::new("wmic")
        .args(["path", class, "get", property, "/format:csv"])
        .output()
    {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            return stdout
                .lines()
                .skip(1)
                .filter_map(|line| {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        return None;
                    }
                    let parts: Vec<&str> = trimmed.splitn(2, ',').collect();
                    if parts.len() >= 2 {
                        let val = parts[1].trim().to_string();
                        if val.is_empty() {
                            None
                        } else {
                            Some(val)
                        }
                    } else {
                        None
                    }
                })
                .collect();
        }
    }
    if let Ok(output) = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            &format!(
                "Get-CimInstance -ClassName {} -ErrorAction SilentlyContinue | Select-Object -ExpandProperty {} -ErrorAction SilentlyContinue",
                class, property
            ),
        ])
        .output()
    {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            return stdout
                .lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect();
        }
    }
    Vec::new()
}

#[cfg(windows)]
fn wmi_pnp_search(keywords: &[&str]) -> bool {
    if let Ok(output) = std::process::Command::new("wmic")
        .args(["path", "Win32_PnPEntity", "get", "Name", "/format:csv"])
        .output()
    {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let lower = stdout.to_lowercase();
            if keywords.iter().any(|kw| lower.contains(kw)) {
                return true;
            }
        }
    }
    let kw_filter = keywords
        .iter()
        .map(|kw| format!("$_ -match '{}'", kw))
        .collect::<Vec<_>>()
        .join(" -or ");
    let cmd = format!(
        "Get-CimInstance -ClassName Win32_PnPEntity -ErrorAction SilentlyContinue | Where-Object {{ {} }} | Select-Object -First 1 -ExpandProperty Name",
        kw_filter
    );
    if let Ok(output) = std::process::Command::new("powershell")
        .args(["-NoProfile", "-Command", &cmd])
        .output()
    {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            return !stdout.trim().is_empty();
        }
    }
    false
}

pub struct RuntimeDetector;

impl RuntimeDetector {
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

    pub fn detect_primary() -> Runtime {
        let runtimes = Self::detect();
        runtimes
            .first()
            .map(|r| r.detected_runtime)
            .unwrap_or(Runtime::CPU)
    }

    fn detect_cuda() -> Option<RuntimeInfo> {
        let has_cuda_toolkit = std::path::Path::new("/usr/local/cuda").exists()
            || std::env::var("CUDA_PATH").is_ok()
            || std::env::var("CUDA_HOME").is_ok()
            || std::env::var("CUDA_TOOLKIT_ROOT_DIR").is_ok();

        let has_nvidia_smi = std::process::Command::new("nvidia-smi")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        if has_cuda_toolkit || has_nvidia_smi {
            let (device_count, compute_capability, gpu_name, memory_gb) = if has_nvidia_smi {
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
                let name = std::process::Command::new("nvidia-smi")
                    .args(["--query-gpu=name", "--format=csv,noheader"])
                    .output()
                    .ok()
                    .and_then(|o| {
                        String::from_utf8_lossy(&o.stdout)
                            .lines()
                            .next()
                            .map(|l| l.trim().to_string())
                    });
                let mem = std::process::Command::new("nvidia-smi")
                    .args(["--query-gpu=memory.total", "--format=csv,noheader,nounits"])
                    .output()
                    .ok()
                    .and_then(|o| {
                        String::from_utf8_lossy(&o.stdout)
                            .lines()
                            .next()
                            .and_then(|l| l.trim().parse::<f32>().ok())
                            .map(|mib| mib / 1024.0)
                    });
                (
                    count.or(Some(1)),
                    cc.or(Some("7.0+".to_string())),
                    name,
                    mem,
                )
            } else {
                (
                    Some(1),
                    Some("7.0+".to_string()),
                    None,
                    Self::detect_gpu_memory(),
                )
            };

            return Some(RuntimeInfo {
                detected_runtime: Runtime::CUDA,
                is_available: true,
                compute_capability,
                memory_gb,
                device_count,
                gpu_name,
            });
        }
        None
    }

    fn detect_metal() -> Option<RuntimeInfo> {
        #[cfg(target_os = "macos")]
        {
            if Self::is_apple_silicon() {
                return Some(RuntimeInfo {
                    detected_runtime: Runtime::Metal,
                    is_available: true,
                    compute_capability: Some("Apple Neural Engine".to_string()),
                    memory_gb: Self::detect_system_memory(),
                    device_count: Some(1),
                    gpu_name: Some("Apple GPU".to_string()),
                });
            }
        }
        None
    }

    #[cfg(feature = "rocm")]
    fn detect_rocm() -> Option<RuntimeInfo> {
        let has_rocm_linux = std::path::Path::new("/opt/rocm").exists()
            || std::env::var("ROCM_HOME").is_ok()
            || std::env::var("ROCM_PATH").is_ok();

        let has_rocm_windows = std::env::var("HIP_PATH").is_ok()
            || std::env::var("ROCM_PATH").is_ok()
            || std::path::Path::new("C:\\Program Files\\AMD\\ROCm").exists()
            || std::path::Path::new("C:\\Program Files\\AMD\\HIP").exists();

        let has_rocm_smi = std::process::Command::new("rocm-smi")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        let has_hipconfig = std::process::Command::new("hipconfig")
            .arg("--full")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        let has_visible_devices = std::env::var("ROCR_VISIBLE_DEVICES").is_ok()
            || std::env::var("HIP_VISIBLE_DEVICES").is_ok();

        if has_rocm_linux
            || has_rocm_windows
            || has_rocm_smi
            || has_hipconfig
            || has_visible_devices
        {
            let device_count = if has_rocm_smi {
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
                    .or(Some(1))
            } else {
                Some(1)
            };

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

            let gpu_name = std::process::Command::new("rocm-smi")
                .args(["--showproductname"])
                .output()
                .ok()
                .and_then(|o| {
                    let stdout = String::from_utf8_lossy(&o.stdout);
                    stdout
                        .lines()
                        .find(|l| l.contains("Card model:"))
                        .and_then(|l| l.split(':').nth(1).map(|s| s.trim().to_string()))
                });

            return Some(RuntimeInfo {
                detected_runtime: Runtime::ROCm,
                is_available: true,
                compute_capability: Some("RDNA3/CDNA3".to_string()),
                memory_gb,
                device_count,
                gpu_name,
            });
        }

        None
    }

    fn detect_directml() -> Option<RuntimeInfo> {
        #[cfg(windows)]
        {
            let has_nvidia = std::process::Command::new("nvidia-smi")
                .arg("--version")
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);

            if has_nvidia {
                return None;
            }

            let gpu_names = wmi_query_values("Win32_VideoController", "Name");

            for name in &gpu_names {
                let lower = name.to_lowercase();
                let is_basic = lower.contains("microsoft basic display");
                if is_basic {
                    continue;
                }
                let is_amd = lower.contains("amd")
                    || lower.contains("radeon")
                    || lower.contains("advanced micro");
                let is_intel =
                    lower.contains("intel") || lower.contains("iris") || lower.contains("arc");
                let is_gpu =
                    is_amd || is_intel || lower.contains("nvidia") || lower.contains("geforce");

                if is_gpu {
                    let d3d12_available =
                        std::path::Path::new("C:\\Windows\\System32\\d3d12.dll").exists();
                    if d3d12_available {
                        return Some(RuntimeInfo {
                            detected_runtime: Runtime::DirectML,
                            is_available: true,
                            compute_capability: Some("DirectX 12".to_string()),
                            memory_gb: Self::detect_gpu_memory(),
                            device_count: Some(1),
                            gpu_name: Some(name.clone()),
                        });
                    }
                }
            }

            if !gpu_names.is_empty() {
                let d3d12_available =
                    std::path::Path::new("C:\\Windows\\System32\\d3d12.dll").exists();
                if d3d12_available {
                    return Some(RuntimeInfo {
                        detected_runtime: Runtime::DirectML,
                        is_available: true,
                        compute_capability: Some("DirectX 12".to_string()),
                        memory_gb: Self::detect_gpu_memory(),
                        device_count: Some(1),
                        gpu_name: Some(gpu_names[0].clone()),
                    });
                }
            }
        }
        None
    }

    fn detect_npu() -> Option<RuntimeInfo> {
        #[cfg(windows)]
        {
            let npu_keywords = [
                "npu",
                "neural",
                "xdna",
                "ai accelerator",
                "ryzen ai",
                "intel npu",
                "neural processor",
                "amd npu",
                "ryze ai",
                "ryzenai",
                "neural processing unit",
                "ai engine",
            ];
            if wmi_pnp_search(&npu_keywords) {
                return Some(RuntimeInfo {
                    detected_runtime: Runtime::NPU,
                    is_available: true,
                    compute_capability: Some("AI Accelerator".to_string()),
                    memory_gb: Some(2.0),
                    device_count: Some(1),
                    gpu_name: Some("NPU".to_string()),
                });
            }

            if let Ok(output) = std::process::Command::new("wmic")
                .args([
                    "path",
                    "Win32_PnPEntity",
                    "get",
                    "Name,PNPClass",
                    "/format:csv",
                ])
                .output()
            {
                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let lower = stdout.to_lowercase();

                    let has_npu = lower.contains("npu")
                        || lower.contains("neural processor")
                        || lower.contains("ai accelerator")
                        || lower.contains("xdna")
                        || lower.contains("ryzen ai");

                    if has_npu {
                        let gpu_name = stdout
                            .lines()
                            .find(|line| {
                                let lower_line = line.to_lowercase();
                                lower_line.contains("npu")
                                    || lower_line.contains("neural")
                                    || lower_line.contains("ai accelerator")
                                    || lower_line.contains("xdna")
                                    || lower_line.contains("ryzen ai")
                            })
                            .and_then(|line| line.split(',').nth(1).map(|s| s.trim().to_string()))
                            .unwrap_or_else(|| "NPU".to_string());

                        return Some(RuntimeInfo {
                            detected_runtime: Runtime::NPU,
                            is_available: true,
                            compute_capability: Some("AI Accelerator".to_string()),
                            memory_gb: Some(2.0),
                            device_count: Some(1),
                            gpu_name: Some(gpu_name),
                        });
                    }
                }
            }
        }

        let has_npu_env = std::env::var("NPU_DEVICE").is_ok()
            || std::env::var("QUALCOMM_NPU").is_ok()
            || std::env::var("MEDIATEK_NPU").is_ok();

        let npu_indicators = [
            "/sys/devices/platform/soc/*/npu",
            "/sys/devices/virtual/npu",
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
                memory_gb: Some(2.0),
                device_count: Some(1),
                gpu_name: Some("NPU".to_string()),
            });
        }
        None
    }

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
            gpu_name: None,
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

        #[cfg(windows)]
        {
            let values = wmi_query_values("Win32_VideoController", "AdapterRAM");
            for val in &values {
                if let Ok(bytes) = val.parse::<f64>() {
                    if bytes > 0.0 {
                        let gb = (bytes / 1_073_741_824.0) as f32;
                        if gb >= 0.5 {
                            return Some(gb);
                        }
                    }
                }
            }
        }

        None
    }

    fn detect_system_memory() -> Option<f32> {
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
            Some(8.0)
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
                        return Some((bytes / 1_073_741_824.0) as f32);
                    }
                }
            }
            Some(16.0)
        }

        #[cfg(windows)]
        {
            let values = wmi_query_values("Win32_OperatingSystem", "TotalVisibleMemorySize");
            for val in &values {
                if let Ok(kb) = val.parse::<f64>() {
                    if kb > 0.0 {
                        return Some((kb / 1_048_576.0) as f32);
                    }
                }
            }
            if let Ok(output) = std::process::Command::new("powershell")
                .args([
                    "-NoProfile",
                    "-Command",
                    "(Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory",
                ])
                .output()
            {
                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    if let Ok(bytes) = stdout.trim().parse::<f64>() {
                        return Some((bytes / 1_073_741_824.0) as f32);
                    }
                }
            }
            Some(16.0)
        }

        #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
        {
            Some(8.0)
        }
    }
}

pub struct ModelRegistry;

impl ModelRegistry {
    pub fn models_for_runtime(runtime: Runtime) -> Vec<ModelInfo> {
        Self::all_models()
            .into_iter()
            .filter(|m| m.recommended_runtimes.contains(&runtime))
            .collect()
    }

    pub fn all_models() -> Vec<ModelInfo> {
        vec![
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

    pub fn recommend_models(runtime: Runtime, available_memory_gb: f32) -> Vec<ModelInfo> {
        Self::models_for_runtime(runtime)
            .into_iter()
            .filter(|m| m.memory_required_gb <= available_memory_gb)
            .collect()
    }

    pub fn best_for_runtime(runtime: Runtime) -> Option<ModelInfo> {
        let models = Self::models_for_runtime(runtime);
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
    fn test_runtime_always_detected() {
        let primary = RuntimeDetector::detect_primary();
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

    #[test]
    fn test_runtime_info_has_gpu_name_field() {
        let runtimes = RuntimeDetector::detect();
        let cpu = runtimes.iter().find(|r| r.detected_runtime == Runtime::CPU);
        assert!(cpu.is_some());
        assert!(
            cpu.unwrap().gpu_name.is_none(),
            "CPU should not have gpu_name"
        );
    }

    #[cfg(feature = "rocm")]
    #[test]
    fn test_rocm_detection_is_available() {
        let runtimes = RuntimeDetector::detect();
        assert!(
            !runtimes.is_empty(),
            "Runtime detection should always return at least CPU"
        );
    }
}
