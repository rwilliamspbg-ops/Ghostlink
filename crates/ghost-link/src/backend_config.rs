//! Backend configuration and persistence for compute preferences.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::backend_registry::ComputeBackend;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputeConfig {
    #[serde(default)]
    pub preferred_backend: Option<String>,
    #[serde(default = "default_auto_discover")]
    pub auto_discover: bool,
    #[serde(default = "default_gpu_memory_allocation")]
    pub gpu_memory_allocation: f32,
    #[serde(default = "default_request_drain_timeout_secs")]
    pub request_drain_timeout_secs: u64,
}

fn default_auto_discover() -> bool {
    true
}

fn default_gpu_memory_allocation() -> f32 {
    0.80
}

fn default_request_drain_timeout_secs() -> u64 {
    30
}

impl Default for ComputeConfig {
    fn default() -> Self {
        Self {
            preferred_backend: None,
            auto_discover: default_auto_discover(),
            gpu_memory_allocation: default_gpu_memory_allocation(),
            request_drain_timeout_secs: default_request_drain_timeout_secs(),
        }
    }
}

impl ComputeConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_preferred_backend(&self) -> Option<ComputeBackend> {
        self.preferred_backend
            .as_deref()
            .and_then(ComputeBackend::from_str)
    }

    pub fn set_preferred_backend(&mut self, backend: ComputeBackend) {
        self.preferred_backend = Some(backend.as_str().to_string());
    }

    pub fn validate(&self) -> Result<(), String> {
        if !(0.0..=1.0).contains(&self.gpu_memory_allocation) {
            return Err(format!(
                "gpu_memory_allocation must be between 0.0 and 1.0, got {}",
                self.gpu_memory_allocation
            ));
        }

        if self.request_drain_timeout_secs == 0 {
            return Err("request_drain_timeout_secs must be > 0".to_string());
        }

        if let Some(backend) = &self.preferred_backend {
            if ComputeBackend::from_str(backend).is_none() {
                return Err(format!("Unknown backend: {}", backend));
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct ConfigManager {
    config_path: PathBuf,
}

impl ConfigManager {
    pub fn new(config_path: impl AsRef<Path>) -> Self {
        Self {
            config_path: config_path.as_ref().to_path_buf(),
        }
    }

    pub fn load_compute_config(&self) -> Result<ComputeConfig, String> {
        if !self.config_path.exists() {
            return Ok(ComputeConfig::default());
        }

        let content = std::fs::read_to_string(&self.config_path)
            .map_err(|err| format!("Failed to read config file: {}", err))?;

        let root: toml::Value =
            toml::from_str(&content).map_err(|err| format!("Failed to parse TOML: {}", err))?;

        if let Some(compute) = root.get("compute") {
            let config: ComputeConfig = compute
                .clone()
                .try_into()
                .map_err(|err| format!("Failed to parse [compute] section: {}", err))?;
            config.validate()?;
            Ok(config)
        } else {
            Ok(ComputeConfig::default())
        }
    }

    pub fn save_preferred_backend(&self, backend: ComputeBackend) -> Result<(), String> {
        let mut config = self.load_compute_config().unwrap_or_default();
        config.set_preferred_backend(backend);
        self.save_compute_config(&config)
    }

    pub fn load_preferred_backend(&self) -> Result<Option<ComputeBackend>, String> {
        Ok(self.load_compute_config()?.get_preferred_backend())
    }

    pub fn save_compute_config(&self, config: &ComputeConfig) -> Result<(), String> {
        config.validate()?;

        let mut root = if self.config_path.exists() {
            let content = std::fs::read_to_string(&self.config_path)
                .map_err(|err| format!("Failed to read config file: {}", err))?;
            toml::from_str::<toml::Value>(&content)
                .map_err(|err| format!("Failed to parse TOML: {}", err))?
                .as_table()
                .cloned()
                .unwrap_or_default()
        } else {
            toml::map::Map::new()
        };

        let compute_value = toml::Value::try_from(config)
            .map_err(|err| format!("Failed to serialize compute config: {}", err))?;
        root.insert("compute".to_string(), compute_value);

        let output = toml::to_string_pretty(&toml::Value::Table(root))
            .map_err(|err| format!("Failed to serialize TOML: {}", err))?;
        std::fs::write(&self.config_path, output)
            .map_err(|err| format!("Failed to write config file: {}", err))?;
        Ok(())
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub struct CLIOverrides {
    pub preferred_backend: Option<String>,
}

#[allow(dead_code)]
impl CLIOverrides {
    pub fn new() -> Self {
        Self {
            preferred_backend: None,
        }
    }

    pub fn from_args(args: &[String]) -> Self {
        let mut overrides = Self::new();
        let mut index = 0;

        while index < args.len() {
            match args[index].as_str() {
                "--backend" => {
                    if let Some(next) = args.get(index + 1) {
                        overrides.preferred_backend = Some(next.clone());
                        index += 2;
                    } else {
                        index += 1;
                    }
                }
                _ if args[index].starts_with("--backend=") => {
                    overrides.preferred_backend =
                        Some(args[index].trim_start_matches("--backend=").to_string());
                    index += 1;
                }
                _ => index += 1,
            }
        }

        overrides
    }

    pub fn get_effective_backend(
        &self,
        config_backend: Option<ComputeBackend>,
    ) -> Option<ComputeBackend> {
        if let Some(backend_str) = &self.preferred_backend {
            return ComputeBackend::from_str(backend_str);
        }

        config_backend
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_config_path() -> PathBuf {
        let suffix = format!(
            "ghostlink-compute-{}-{}.toml",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        std::env::temp_dir().join(suffix)
    }

    #[test]
    fn test_compute_config_defaults() {
        let config = ComputeConfig::default();
        assert!(config.auto_discover);
        assert_eq!(config.gpu_memory_allocation, 0.80);
        assert_eq!(config.request_drain_timeout_secs, 30);
        assert!(config.preferred_backend.is_none());
    }

    #[test]
    fn test_compute_config_validate() {
        let mut config = ComputeConfig::new();
        assert!(config.validate().is_ok());

        config.gpu_memory_allocation = 1.5;
        assert!(config.validate().is_err());

        config.gpu_memory_allocation = 0.80;
        config.request_drain_timeout_secs = 0;
        assert!(config.validate().is_err());

        config.request_drain_timeout_secs = 30;
        config.preferred_backend = Some("invalid".to_string());
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_compute_config_set_backend() {
        let mut config = ComputeConfig::new();
        assert!(config.get_preferred_backend().is_none());

        config.set_preferred_backend(ComputeBackend::Rocm);
        assert_eq!(config.get_preferred_backend(), Some(ComputeBackend::Rocm));
    }

    #[test]
    fn test_config_manager_save_and_load_preferred_backend() {
        let path = temp_config_path();
        let manager = ConfigManager::new(&path);

        manager
            .save_preferred_backend(ComputeBackend::Rocm)
            .unwrap();

        let loaded = manager.load_preferred_backend().unwrap();
        assert_eq!(loaded, Some(ComputeBackend::Rocm));

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_cli_overrides_from_args() {
        let args = [
            "cmd".to_string(),
            "--backend".to_string(),
            "cpu".to_string(),
            "other".to_string(),
        ];

        let overrides = CLIOverrides::from_args(&args[1..]);
        assert_eq!(overrides.preferred_backend, Some("cpu".to_string()));
    }

    #[test]
    fn test_cli_overrides_equals_syntax() {
        let args = vec!["--backend=rocm".to_string()];

        let overrides = CLIOverrides::from_args(&args);
        assert_eq!(overrides.preferred_backend, Some("rocm".to_string()));
    }

    #[test]
    fn test_cli_overrides_get_effective_backend() {
        let mut overrides = CLIOverrides::new();

        assert!(overrides.get_effective_backend(None).is_none());
        assert_eq!(
            overrides.get_effective_backend(Some(ComputeBackend::Rocm)),
            Some(ComputeBackend::Rocm)
        );

        overrides.preferred_backend = Some("cpu".to_string());
        assert_eq!(
            overrides.get_effective_backend(Some(ComputeBackend::Rocm)),
            Some(ComputeBackend::Cpu)
        );
    }

    #[test]
    fn test_compute_config_serialization() {
        let mut config = ComputeConfig::new();
        config.set_preferred_backend(ComputeBackend::Rocm);

        let toml_str = toml::to_string(&config).unwrap();
        assert!(toml_str.contains("rocm"));
        assert!(toml_str.contains("0.8"));

        let parsed: ComputeConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.get_preferred_backend(), Some(ComputeBackend::Rocm));
    }

    #[test]
    fn test_config_manager_path_handling() {
        let config_path = PathBuf::from("/tmp/test_ghostlink.toml");
        let manager = ConfigManager::new(&config_path);
        assert_eq!(manager.config_path, config_path);
    }

    #[test]
    fn test_cli_overrides_no_backend() {
        let args = ["cmd".to_string(), "--other".to_string()];

        let overrides = CLIOverrides::from_args(&args[1..]);
        assert!(overrides.preferred_backend.is_none());
    }
}
