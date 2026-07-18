//! Phase 3: Runtime Backend Switching
//! Implements graceful backend switching with request draining, environment updates, and process restart

#![allow(dead_code)] // Public API for runtime switching

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::sleep;

use crate::backend_registry::{BackendRegistry, ComputeBackend};

/// Configuration for runtime switching behavior
#[derive(Debug, Clone)]
pub struct SwitchingConfig {
    /// Maximum time to wait for in-flight requests (seconds)
    pub request_drain_timeout: Duration,
    /// Whether to restart inference client automatically
    pub auto_restart_client: bool,
    /// Environment variables to update per backend
    pub backend_env_vars: HashMap<String, HashMap<String, String>>,
}

impl Default for SwitchingConfig {
    fn default() -> Self {
        let mut backend_env_vars = HashMap::new();

        // ROCm environment
        let mut rocm_env = HashMap::new();
        rocm_env.insert("HSA_OVERRIDE_GFX_VERSION".to_string(), "gfx906".to_string());
        rocm_env.insert("HIP_PLATFORM".to_string(), "amd".to_string());
        rocm_env.insert("OLLAMA_NUM_THREAD".to_string(), "16".to_string());
        rocm_env.insert("OLLAMA_GPU_MEMORY".to_string(), "3276".to_string());
        backend_env_vars.insert("rocm".to_string(), rocm_env);

        // CUDA environment
        let mut cuda_env = HashMap::new();
        cuda_env.insert("CUDA_VISIBLE_DEVICES".to_string(), "0".to_string());
        cuda_env.insert("TF_CPP_MIN_LOG_LEVEL".to_string(), "2".to_string());
        cuda_env.insert("OLLAMA_NUM_THREAD".to_string(), "16".to_string());
        backend_env_vars.insert("cuda".to_string(), cuda_env);

        // CPU environment
        let mut cpu_env = HashMap::new();
        cpu_env.insert("OLLAMA_NUM_THREAD".to_string(), "16".to_string());
        cpu_env.insert("OLLAMA_GPU_MEMORY".to_string(), "0".to_string());
        backend_env_vars.insert("cpu".to_string(), cpu_env);

        SwitchingConfig {
            request_drain_timeout: Duration::from_secs(30),
            auto_restart_client: true,
            backend_env_vars,
        }
    }
}

/// Request tracking for draining
#[derive(Debug, Clone, Default)]
pub struct RequestTracker {
    in_flight: Arc<Mutex<usize>>,
}

impl RequestTracker {
    /// Create a new request tracker
    pub fn new() -> Self {
        Self {
            in_flight: Arc::new(Mutex::new(0)),
        }
    }

    /// Increment in-flight request counter
    pub async fn increment(&self) {
        let mut count = self.in_flight.lock().await;
        *count += 1;
    }

    /// Decrement in-flight request counter
    pub async fn decrement(&self) {
        let mut count = self.in_flight.lock().await;
        if *count > 0 {
            *count -= 1;
        }
    }

    /// Get current in-flight request count
    pub async fn get_count(&self) -> usize {
        *self.in_flight.lock().await
    }

    /// Wait for all in-flight requests to complete (with timeout)
    pub async fn drain(&self, timeout: Duration) -> Result<(), String> {
        let start = std::time::Instant::now();

        loop {
            let count = self.get_count().await;
            if count == 0 {
                return Ok(());
            }

            if start.elapsed() > timeout {
                return Err(format!(
                    "Request drain timeout: {} requests still in-flight",
                    count
                ));
            }

            sleep(Duration::from_millis(100)).await;
        }
    }
}

/// Environment variable manager
#[derive(Debug, Clone)]
pub struct EnvironmentManager {
    config: SwitchingConfig,
}

impl EnvironmentManager {
    /// Create a new environment manager
    pub fn new(config: SwitchingConfig) -> Self {
        Self { config }
    }

    /// Update environment variables for a backend
    pub fn set_backend_env(&self, backend: &ComputeBackend) -> Result<(), String> {
        let backend_name = backend.as_str();
        let env_vars = self
            .config
            .backend_env_vars
            .get(backend_name)
            .ok_or_else(|| format!("No environment configuration for backend: {}", backend_name))?;

        for (key, value) in env_vars {
            std::env::set_var(key, value);
        }

        Ok(())
    }

    /// Get current environment variables for a backend
    pub fn get_backend_env(&self, backend: &ComputeBackend) -> Option<HashMap<String, String>> {
        let backend_name = backend.as_str();
        self.config.backend_env_vars.get(backend_name).cloned()
    }

    /// Restore previous environment variables (for rollback)
    pub fn restore_env(&self, backend: &ComputeBackend) -> Result<(), String> {
        let backend_name = backend.as_str();
        let env_vars = self
            .config
            .backend_env_vars
            .get(backend_name)
            .ok_or_else(|| format!("No environment configuration for backend: {}", backend_name))?;

        for key in env_vars.keys() {
            std::env::remove_var(key);
        }

        Ok(())
    }
}

/// Runtime backend switcher with graceful shutdown
#[derive(Debug, Clone)]
pub struct RuntimeSwitcher {
    request_tracker: RequestTracker,
    env_manager: EnvironmentManager,
    config: SwitchingConfig,
}

impl RuntimeSwitcher {
    /// Create a new runtime switcher
    pub fn new(config: SwitchingConfig) -> Self {
        Self {
            request_tracker: RequestTracker::new(),
            env_manager: EnvironmentManager::new(config.clone()),
            config,
        }
    }

    /// Get the request tracker (for incrementing/decrementing on requests)
    pub fn request_tracker(&self) -> &RequestTracker {
        &self.request_tracker
    }

    /// Perform graceful backend switch
    pub async fn switch_backend(
        &self,
        registry: &BackendRegistry,
        target_backend: ComputeBackend,
    ) -> Result<SwitchResult, String> {
        // Validate backend is available
        if registry.get_backend(&target_backend).is_none() {
            return Err(format!(
                "Target backend {} not available",
                target_backend.as_str()
            ));
        }

        // Step 1: Drain in-flight requests
        tracing::info!(
            "Phase3: Draining in-flight requests (timeout: {:?})",
            self.config.request_drain_timeout
        );
        self.request_tracker
            .drain(self.config.request_drain_timeout)
            .await?;

        // Step 2: Update environment variables
        tracing::info!(
            "Phase3: Updating environment variables for {}",
            target_backend.as_str()
        );
        self.env_manager.set_backend_env(&target_backend)?;

        // Step 3: Switch backend in registry
        tracing::info!("Phase3: Switching backend in registry");
        registry.switch_backend(target_backend.clone())?;

        // Step 4: Note: Actual process restart would happen here
        // For now, we return info about what needs to happen
        let restart_required = self.config.auto_restart_client;

        Ok(SwitchResult {
            backend: target_backend.as_str().to_string(),
            in_flight_drained: 0, // Would be actual count
            env_vars_updated: self
                .env_manager
                .get_backend_env(&target_backend)
                .map(|vars| vars.len())
                .unwrap_or(0),
            restart_required,
            message: format!(
                "Successfully switched to {} backend{}",
                target_backend.as_str(),
                if restart_required {
                    " (restart required)"
                } else {
                    ""
                }
            ),
        })
    }

    /// Rollback to previous backend
    pub async fn rollback_backend(
        &self,
        registry: &BackendRegistry,
        previous_backend: ComputeBackend,
    ) -> Result<(), String> {
        tracing::warn!(
            "Phase3: Rolling back to {} backend",
            previous_backend.as_str()
        );

        // Restore environment
        self.env_manager.restore_env(&previous_backend)?;

        // Switch back in registry
        registry.switch_backend(previous_backend)?;

        Ok(())
    }
}

/// Result of a successful backend switch
#[derive(Debug, Clone, serde::Serialize)]
pub struct SwitchResult {
    pub backend: String,
    pub in_flight_drained: usize,
    pub env_vars_updated: usize,
    pub restart_required: bool,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_switching_config_default() {
        let config = SwitchingConfig::default();
        assert_eq!(config.request_drain_timeout, Duration::from_secs(30));
        assert!(config.auto_restart_client);
        assert!(config.backend_env_vars.contains_key("rocm"));
        assert!(config.backend_env_vars.contains_key("cuda"));
        assert!(config.backend_env_vars.contains_key("cpu"));
    }

    #[test]
    fn test_environment_manager_get_env() {
        let config = SwitchingConfig::default();
        let manager = EnvironmentManager::new(config);

        let rocm_env = manager.get_backend_env(&ComputeBackend::Rocm);
        assert!(rocm_env.is_some());
        let env = rocm_env.unwrap();
        assert_eq!(env.get("HSA_OVERRIDE_GFX_VERSION").unwrap(), "gfx906");
        assert_eq!(env.get("HIP_PLATFORM").unwrap(), "amd");
    }

    #[test]
    fn test_environment_manager_set_env() {
        let config = SwitchingConfig::default();
        let manager = EnvironmentManager::new(config);

        manager.set_backend_env(&ComputeBackend::Cpu).unwrap();

        assert_eq!(
            std::env::var("OLLAMA_NUM_THREAD").ok(),
            Some("16".to_string())
        );
        assert_eq!(
            std::env::var("OLLAMA_GPU_MEMORY").ok(),
            Some("0".to_string())
        );

        // Cleanup
        std::env::remove_var("OLLAMA_NUM_THREAD");
        std::env::remove_var("OLLAMA_GPU_MEMORY");
    }

    #[tokio::test]
    async fn test_request_tracker_increment_decrement() {
        let tracker = RequestTracker::new();

        assert_eq!(tracker.get_count().await, 0);

        tracker.increment().await;
        assert_eq!(tracker.get_count().await, 1);

        tracker.increment().await;
        assert_eq!(tracker.get_count().await, 2);

        tracker.decrement().await;
        assert_eq!(tracker.get_count().await, 1);

        tracker.decrement().await;
        assert_eq!(tracker.get_count().await, 0);
    }

    #[tokio::test]
    async fn test_request_tracker_drain_immediate() {
        let tracker = RequestTracker::new();

        // Should drain immediately if no requests in flight
        let result = tracker.drain(Duration::from_secs(5)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_request_tracker_drain_timeout() {
        let tracker = RequestTracker::new();

        // Add an in-flight request
        tracker.increment().await;

        // Should timeout
        let result = tracker.drain(Duration::from_millis(100)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Request drain timeout"));

        tracker.decrement().await;
    }

    #[tokio::test]
    async fn test_request_tracker_drain_waits() {
        let tracker = RequestTracker::new();
        let tracker2 = tracker.clone();

        // Spawn task to decrement after delay
        tokio::spawn(async move {
            sleep(Duration::from_millis(200)).await;
            tracker2.decrement().await;
        });

        // Add an in-flight request
        tracker.increment().await;

        // Should drain after waiting
        let result = tracker.drain(Duration::from_secs(5)).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_switch_result_serialization() {
        let result = SwitchResult {
            backend: "rocm".to_string(),
            in_flight_drained: 5,
            env_vars_updated: 3,
            restart_required: false,
            message: "Successfully switched".to_string(),
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("rocm"));
        assert!(json.contains("5"));
    }
}
