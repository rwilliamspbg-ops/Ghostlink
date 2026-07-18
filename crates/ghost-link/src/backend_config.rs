//! Phase 4: Backend Configuration & Persistence
//! Implements saving/loading backend preferences to/from ghostlink.toml

#![allow(dead_code)] // Public API for config management

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::backend_registry::ComputeBackend;

/// Compute backend configuration section for ghostlink.toml
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ComputeConfig {
    /// Preferred backend (rocm, cuda, oneapi, metal, cpu)
    pub preferred_backend: Option<String>,
    /// Enable automatic backend discovery
    #[serde(default = "default_auto_discover")]
    pub auto_discover: bool,
    /// GPU memory allocation percentage (0.0-1.0)
    #[serde(default = "default_gpu_memory_allocation")]
    pub gpu_memory_allocation: f32,
    /// Request drain timeout in seconds
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

impl ComputeConfig {
    /// Create a new compute configuration with defaults
    pub fn new() -> Self {
        Self {
            preferred_backend: None,
            auto_discover: true,
            gpu_memory_allocation: 0.80,
            request_drain_timeout_secs: 30,
        }
    }

    /// Get the preferred backend if set
    pub fn get_preferred_backend(&self) -> Option<ComputeBackend> {
        self.preferred_backend
            .as_ref()
            .and_then(|name| ComputeBackend::from_str(name))
    }

    /// Set the preferred backend
    pub fn set_preferred_backend(&mut self, backend: ComputeBackend) {
        self.preferred_backend = Some(backend.as_str().to_string());
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<(), String> {
        if self.gpu_memory_allocation < 0.0 || self.gpu_memory_allocation > 1.0 {
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

/// Backend configuration manager
pub struct ConfigManager {
    config_path: PathBuf,
}

impl ConfigManager {
    /// Create a new configuration manager
    pub fn new(config_path: impl AsRef<Path>) -> Self {
        Self {
            config_path: config_path.as_ref().to_path_buf(),
        }
    }

    /// Load compute configuration from file
    pub fn load_compute_config(&self) -> Result<ComputeConfig, String> {
        // Try to read the config file
        let content = std::fs::read_to_string(&self.config_path)
            .map_err(|err| format!("Failed to read config file: {}", err))?;

        // Parse as TOML
        let config: toml::Table = toml::from_str(&content)
            .map_err(|err| format!("Failed to parse TOML: {}", err))?;

        // Extract [compute] section or use defaults
        let compute_config = if let Some(compute) = config.get("compute") {
            compute
                .as_table()
                .ok_or("[compute] section must be a table".to_string())?
                .clone()
                .try_into::<ComputeConfig>()
                .map_err(|err| format!("Failed to parse [compute] section: {}", err))?
        } else {
            ComputeConfig::new()
        };

        compute_config.validate()?;
        Ok(compute_config)
    }

    /// Save compute configuration to file
    pub fn save_compute_config(&self, config: &ComputeConfig) -> Result<(), String> {
        config.validate()?;

        // Read existing config or create new
        let mut root: toml::Table = if self.config_path.exists() {
            let content = std::fs::read_to_string(&self.config_path)
                .map_err(|err| format!("Failed to read config file: {}", err))?;

            toml::from_str(&content)
                .map_err(|err| format!("Failed to parse TOML: {}", err))?
        } else {
            toml::Table::new()
        };

        // Serialize config to table
        let config_json = serde_json::to_value(config)
            .map_err(|err| format!("Failed to convert config: {}", err))?;

        let compute_table = config_json
            .as_object()
            .ok_or("Failed to serialize compute config".to_string())?
            .iter()
            .map(|(k, v)| {
                let val = match v {
                    serde_json::Value::String(s) => toml::Value::String(s.clone()),
                    serde_json::Value::Bool(b) => toml::Value::Boolean(*b),
                    serde_json::Value::Number(n) => {
                        if let Some(f) = n.as_f64() {
                            toml::Value::Float(f)
                        } else if let Some(i) = n.as_i64() {
                            toml::Value::Integer(i)
                        } else {
                            return Err("Invalid number".to_string());
                        }
                    }
                    serde_json::Value::Null => toml::Value::String(String::new()),
                    _ => return Err("Unsupported value type".to_string()),
                };
                Ok((k.clone(), val))
            })
            .collect::<Result<toml::Table, String>>()?;

        root.insert("compute".to_string(), toml::Value::Table(compute_table));

        // Write back to file
        let output = toml::to_string_pretty(&root)
            .map_err(|err| format!("Failed to serialize TOML: {}", err))?;

        std::fs::write(&self.config_path, output)
            .map_err(|err| format!("Failed to write config file: {}", err))?;

        tracing::info!(
            "Phase4: Saved compute config to {}",
            self.config_path.display()
        );

        Ok(())
    }

    /// Load preferred backend from config
    pub fn load_preferred_backend(&self) -> Result<Option<ComputeBackend>, String> {
        let config = self.load_compute_config()?;
        Ok(config.get_preferred_backend())
    }

    /// Save preferred backend to config
    pub fn save_preferred_backend(&self, backend: ComputeBackend) -> Result<(), String> {
        let mut config = self.load_compute_config().unwrap_or_else(|_| ComputeConfig::new());
        config.set_preferred_backend(backend);
        self.save_compute_config(&config)
    }
}

/// Environment variable overrides from CLI
#[derive(Debug, Clone, Default)]
pub struct CLIOverrides {
    /// Override preferred backend from command line
    pub preferred_backend: Option<String>,
}

impl CLIOverrides {
    /// Create new CLI overrides
    pub fn new() -> Self {
        Self {
            preferred_backend: None,
        }
    }

    /// Parse CLI arguments for backend override
    pub fn from_args(args: &[String]) -> Self {
        let mut overrides = Self::new();

        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "--backend" => {
                    if let Some(next) = args.get(i + 1) {
                        overrides.preferred_backend = Some(next.clone());
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
                _ if args[i].starts_with("--backend=") => {
                    overrides.preferred_backend =
                        Some(args[i].trim_start_matches("--backend=").to_string());
                    i += 1;
                }
                _ => i += 1,
            }
        }

        overrides
    }

    /// Get effective backend (CLI override takes precedence)
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

    #[test]
    fn test_compute_config_new() {
        let config = ComputeConfig::new();
        assert_eq!(config.gpu_memory_allocation, 0.80);
        assert_eq!(config.request_drain_timeout_secs, 30);
        assert!(config.auto_discover);
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
        assert_eq!(
            config.get_preferred_backend(),
            Some(ComputeBackend::Rocm)
        );
    }

    #[test]
    fn test_cli_overrides_from_args() {
        let args = vec![
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

        // No override, no config
        assert!(overrides
            .get_effective_backend(None)
            .is_none());

        // No override, with config
        assert_eq!(
            overrides.get_effective_backend(Some(ComputeBackend::Rocm)),
            Some(ComputeBackend::Rocm)
        );

        // Override takes precedence
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
        let args = vec!["cmd".to_string(), "--other".to_string()];

        let overrides = CLIOverrides::from_args(&args[1..]);
        assert!(overrides.preferred_backend.is_none());
    }

    #[test]
    fn test_compute_config_defaults() {
        let config = ComputeConfig::new();
        assert_eq!(default_auto_discover(), true);
        assert_eq!(default_gpu_memory_allocation(), 0.80);
        assert_eq!(default_request_drain_timeout_secs(), 30);
    }
}
