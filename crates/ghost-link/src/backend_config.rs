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
}
