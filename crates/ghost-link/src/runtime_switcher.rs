//! Phase 3: Runtime Backend Switching
//! Implements graceful backend switching with request draining, environment updates, and process restart

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore};
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

/// Request tracking for draining, plus admission control matching however
/// many concurrent slots the currently-running llama-server actually has.
#[derive(Debug, Clone)]
pub struct RequestTracker {
    in_flight: Arc<Mutex<usize>>,
    slot_semaphore: Arc<Semaphore>,
    /// `Semaphore` only exposes *currently free* permits
    /// (`available_permits()`), not the configured total — tracked
    /// separately so `resize_slots` can compute a correct delta even while
    /// permits are checked out by in-flight requests.
    slot_capacity: Arc<AtomicUsize>,
}

impl Default for RequestTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl RequestTracker {
    /// Create a new request tracker. Slot capacity starts at whatever
    /// `GHOSTLINK_PARALLEL_SLOTS` currently says (same env var
    /// `native_engine::get_parallel_slots()` reads for llama-server's `-np`,
    /// defaulting to `1` — today's exact prior behavior — when unset), so a
    /// tracker constructed after `load_settings()` mirrors persisted
    /// settings into that env var starts in sync without an extra call.
    pub fn new() -> Self {
        let initial_slots = std::env::var("GHOSTLINK_PARALLEL_SLOTS")
            .ok()
            .and_then(|v| v.trim().parse::<usize>().ok())
            .map(|n| n.clamp(1, 64))
            .unwrap_or(1);
        Self {
            in_flight: Arc::new(Mutex::new(0)),
            slot_semaphore: Arc::new(Semaphore::new(initial_slots)),
            slot_capacity: Arc::new(AtomicUsize::new(initial_slots)),
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

    /// Wait for a free llama-server slot before admitting a new generation
    /// request — bounds real concurrent calls into llama-server to whatever
    /// `resize_slots`/construction last configured, instead of firing
    /// unbounded concurrent HTTP requests at a process that may only have
    /// one real inference slot. The returned permit releases automatically
    /// when dropped; callers just need to keep it alive for the duration of
    /// the generation call.
    pub async fn acquire_slot(&self) -> OwnedSemaphorePermit {
        self.slot_semaphore
            .clone()
            .acquire_owned()
            .await
            .expect("slot semaphore is never closed")
    }

    /// Real `/api/queue` depth: requests already admitted into ghost-link
    /// (counted via `increment`, called before `acquire_slot`) but still
    /// waiting for a free llama-server slot — `tokio::sync::Semaphore`
    /// doesn't expose a waiter count directly, so this derives it as
    /// `in_flight - requests_actively_holding_a_slot`, both of which are
    /// real, already-tracked numbers rather than an estimate.
    pub async fn queue_depth(&self) -> usize {
        let in_flight = self.get_count().await;
        let active_in_slot = self
            .slot_capacity()
            .saturating_sub(self.slot_semaphore.available_permits());
        in_flight.saturating_sub(active_in_slot)
    }

    /// Current configured slot capacity.
    pub fn slot_capacity(&self) -> usize {
        self.slot_capacity.load(Ordering::SeqCst)
    }

    /// Resize the slot semaphore to match a new `parallel_slots` setting.
    /// Safe to call at any time, including while permits are checked out —
    /// a shrink below what's currently available only fully applies once
    /// enough in-flight requests finish and return their permits.
    pub fn resize_slots(&self, new_capacity: usize) {
        let new_capacity = new_capacity.clamp(1, 64);
        let old_capacity = self.slot_capacity.swap(new_capacity, Ordering::SeqCst);
        match new_capacity.cmp(&old_capacity) {
            std::cmp::Ordering::Greater => {
                self.slot_semaphore.add_permits(new_capacity - old_capacity);
            }
            std::cmp::Ordering::Less => {
                self.slot_semaphore
                    .forget_permits(old_capacity - new_capacity);
            }
            std::cmp::Ordering::Equal => {}
        }
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
                    "Request drain timeout: {count} requests still in-flight"
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

    /// Update environment variables for a backend.
    ///
    /// A backend with no entry in `backend_env_vars` (metal, oneapi,
    /// directml, vulkan, npu — none of which need ghost-link to set special
    /// env vars the way ROCm/CUDA do) is not an error: it just means there's
    /// nothing to set. Previously this returned Err for any such backend,
    /// which meant `/api/backends/switch` always failed for GPUs discovered
    /// via any of those paths even though the registry correctly reported
    /// them as available — the backend was listed but never actually
    /// selectable.
    pub fn set_backend_env(&self, backend: &ComputeBackend) -> Result<(), String> {
        let backend_name = backend.as_str();
        let Some(env_vars) = self.config.backend_env_vars.get(backend_name) else {
            return Ok(());
        };

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

    /// Restore previous environment variables (for rollback). Same "missing
    /// entry means nothing to do" reasoning as set_backend_env.
    pub fn restore_env(&self, backend: &ComputeBackend) -> Result<(), String> {
        let backend_name = backend.as_str();
        let Some(env_vars) = self.config.backend_env_vars.get(backend_name) else {
            return Ok(());
        };

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

        // Step 2: Clear stale backend-specific environment variables
        for backend_name in self.config.backend_env_vars.keys() {
            if let Some(backend) = ComputeBackend::from_str(backend_name) {
                let _ = self.env_manager.restore_env(&backend);
            }
        }

        // Step 3: Update environment variables
        tracing::info!(
            "Phase3: Updating environment variables for {}",
            target_backend.as_str()
        );
        self.env_manager.set_backend_env(&target_backend)?;

        // Step 4: Switch backend in registry
        tracing::info!("Phase3: Switching backend in registry");
        registry.switch_backend(target_backend)?;

        // Step 5: Note: Actual process restart would happen here
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

/// Serializes tests (in this module and in `backend_api`) that mutate
/// process-global environment variables via `EnvironmentManager`/
/// `RuntimeSwitcher`. Env vars are process-global, not thread-local, and
/// `cargo test` runs tests within a binary in parallel by default, so
/// without this lock two such tests can interleave their `set_var`/
/// `remove_var` calls and read back each other's values.
///
/// A `tokio::sync::Mutex` is used (rather than `std::sync::Mutex`) so async
/// tests can hold the guard across `.await` points; sync tests use
/// `blocking_lock()` instead.
#[cfg(test)]
pub(crate) fn env_test_lock() -> &'static tokio::sync::Mutex<()> {
    use std::sync::OnceLock;
    static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
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
        let _guard = env_test_lock().blocking_lock();

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

    #[tokio::test]
    async fn test_request_tracker_default_slot_capacity_is_one() {
        let _guard = env_test_lock().lock().await;
        std::env::remove_var("GHOSTLINK_PARALLEL_SLOTS");

        let tracker = RequestTracker::new();
        assert_eq!(tracker.slot_capacity(), 1);

        let _permit = tracker.acquire_slot().await;
        // A second concurrent acquire on a 1-slot tracker must not resolve
        // immediately — proven by racing it against a short timeout.
        let second = tokio::time::timeout(Duration::from_millis(50), tracker.acquire_slot()).await;
        assert!(
            second.is_err(),
            "second acquire should block while the only slot is held"
        );
    }

    #[tokio::test]
    async fn test_request_tracker_reads_initial_capacity_from_env() {
        let _guard = env_test_lock().lock().await;
        std::env::set_var("GHOSTLINK_PARALLEL_SLOTS", "3");

        let tracker = RequestTracker::new();
        assert_eq!(tracker.slot_capacity(), 3);

        std::env::remove_var("GHOSTLINK_PARALLEL_SLOTS");
    }

    #[tokio::test]
    async fn test_request_tracker_resize_slots_grows_and_shrinks() {
        let _guard = env_test_lock().lock().await;
        std::env::remove_var("GHOSTLINK_PARALLEL_SLOTS");

        let tracker = RequestTracker::new();
        assert_eq!(tracker.slot_capacity(), 1);

        tracker.resize_slots(4);
        assert_eq!(tracker.slot_capacity(), 4);

        // All 4 slots should now be independently acquirable without blocking.
        let permits: Vec<_> =
            futures::future::join_all((0..4).map(|_| tracker.acquire_slot())).await;
        assert_eq!(permits.len(), 4);

        // Shrinking below what's currently held doesn't panic or under/overflow;
        // it takes effect once permits are returned.
        tracker.resize_slots(1);
        assert_eq!(tracker.slot_capacity(), 1);
        drop(permits);
    }

    #[tokio::test]
    async fn test_request_tracker_queue_depth_reflects_admitted_but_unslotted_requests() {
        let _guard = env_test_lock().lock().await;
        std::env::remove_var("GHOSTLINK_PARALLEL_SLOTS");

        let tracker = RequestTracker::new(); // 1 slot
        assert_eq!(tracker.queue_depth().await, 0);

        tracker.increment().await;
        let _permit = tracker.acquire_slot().await;
        // Admitted and holding the only slot: not queued.
        assert_eq!(tracker.queue_depth().await, 0);

        // A second request is admitted but the slot is taken — it's queued.
        tracker.increment().await;
        assert_eq!(tracker.queue_depth().await, 1);

        tracker.decrement().await;
        tracker.decrement().await;
    }

    #[test]
    fn test_backend_without_env_config_is_not_an_error() {
        // Regression: metal/oneapi/directml/vulkan/npu have no entry in
        // SwitchingConfig::default()'s backend_env_vars (none of them need
        // ghost-link to set special env vars the way ROCm/CUDA do). This used
        // to make set_backend_env/restore_env return Err for any of them,
        // which meant switching to a GPU discovered via those paths always
        // failed even though the registry correctly listed it as available.
        // Uses Metal specifically (never touches real env vars) so this
        // doesn't add to the process-global env var surface other tests
        // in this file race on.
        let config = SwitchingConfig::default();
        assert!(!config.backend_env_vars.contains_key("metal"));
        let manager = EnvironmentManager::new(config);

        assert!(manager.set_backend_env(&ComputeBackend::Metal).is_ok());
        assert!(manager.restore_env(&ComputeBackend::Metal).is_ok());
        assert!(manager.get_backend_env(&ComputeBackend::Metal).is_none());
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
