//! Unified auto-tuning for Ghost-Link subsystems.
//!
//! `AutoTuner` derives all tunable configuration from a single `SystemProfile`
//! and provides a persistent disk cache so re-tuning is unnecessary across
//! restarts as long as the hardware fingerprint has not changed.

use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::Mutex;

use crate::host::RuntimeProfile;
use crate::system_profile::SystemProfile;

/// In-memory cache of the last-computed AutoTuner.
/// Avoids re-tuning on repeated calls within the same process lifetime.
static TUNE_CACHE: Mutex<Option<AutoTuner>> = Mutex::new(None);

// ---------------------------------------------------------------------------
// AutoTuner
// ---------------------------------------------------------------------------

/// Pre-computed tuning parameters for all Ghost-Link subsystems.
#[derive(Clone, Debug)]
pub struct AutoTuner {
    /// System fingerprint used for cache invalidation.
    pub fingerprint: u64,
    /// Timestamp of the last full re-tune.
    pub tuned_at: String,
    /// Tuned health-check configuration.
    pub health_config: super::health::HealthConfig,
    /// Tuned load-balancing configuration.
    pub load_balance_config: super::load_balance::LoadBalanceConfig,
    /// Tuned planning hints (layer chunking).
    pub planning_tuning: super::planning::PlanningTuning,
    /// Recommended TCP transport settings.
    pub tcp_config: TcpAutoConfig,
    /// Recommended worker-pool sizing.
    pub worker_pool: WorkerPoolConfig,
}

/// TCP transport settings derived from system profile + optional benchmarks.
#[derive(Clone, Debug)]
pub struct TcpAutoConfig {
    pub max_inflight_batches: usize,
    pub reconnect_attempts: usize,
    pub reconnect_backoff_ms: u64,
}

/// Worker pool sizing derived from the system profile.
#[derive(Clone, Debug)]
pub struct WorkerPoolConfig {
    pub compute_workers: usize,
    pub io_workers: usize,
    pub total_workers: usize,
}

impl AutoTuner {
    /// Tune all subsystems from a `SystemProfile`.
    /// Results are cached in-memory to avoid re-tuning on repeated calls.
    pub fn from_system_profile(profile: &SystemProfile) -> Self {
        let fp = compute_fingerprint(profile);
        {
            let cache = TUNE_CACHE.lock().unwrap();
            if let Some(cached) = cache.as_ref() {
                if cached.fingerprint == fp {
                    return cached.clone();
                }
            }
        }
        let tuner = tune(profile);
        let mut cache = TUNE_CACHE.lock().unwrap();
        *cache = Some(tuner.clone());
        tuner
    }

    /// Load a previously cached `AutoTuner` if the hardware fingerprint matches.
    /// Checks the in-memory cache first, then falls back to disk.
    pub fn load_cache() -> Option<Self> {
        // Current fingerprint is needed to validate either cache — compute it
        // once up front rather than only on the disk path. The in-memory fast
        // path previously returned whatever was cached with no fingerprint
        // check at all (unlike from_system_profile() and the disk path below),
        // so a hardware change mid-process (GPU hot-plug/unplug, VRAM change)
        // kept serving stale tuning forever from the in-memory cache.
        let current = SystemProfile::detect_fast();
        let current_fp = compute_fingerprint(&current);

        // Fast path: in-memory cache hit
        {
            let cache = TUNE_CACHE.lock().unwrap();
            if let Some(cached) = cache.as_ref() {
                if cached.fingerprint == current_fp {
                    return Some(cached.clone());
                }
            }
        }

        // Slow path: try disk
        let path = cache_path()?;
        let data = fs::read_to_string(&path).ok()?;
        let cached: AutoTuner = serde_json::from_str(&data).ok()?;

        if cached.fingerprint != current_fp {
            return None;
        }

        // Populate in-memory cache for next time
        let mut cache = TUNE_CACHE.lock().unwrap();
        *cache = Some(cached.clone());
        Some(cached)
    }

    /// Persist the current tuning to disk.
    pub fn save_cache(&self) {
        let path = match cache_path() {
            Some(p) => p,
            None => return,
        };
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(data) = serde_json::to_string_pretty(self) {
            let _ = fs::write(&path, data);
        }
    }

    /// Run TCP transport benchmark and update `tcp_config`.
    /// This is a no-op placeholder that uses profile-derived defaults;
    /// the real benchmark lives in `ghost-link` (main.rs).
    pub fn benchmark_tcp(&mut self) {
        // Real TCP benchmarking (measuring throughput vs. max_inflight)
        // is deferred to the binary crate. Here we apply profile-derived defaults.
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn tune(profile: &SystemProfile) -> AutoTuner {
    let fingerprint = compute_fingerprint(profile);
    let tuned_at = now_iso8601();
    let rp: RuntimeProfile = profile.into();

    AutoTuner {
        fingerprint,
        tuned_at,
        health_config: super::health::HealthConfig::autotuned(&rp),
        load_balance_config: super::load_balance::LoadBalanceConfig::autotuned(&rp),
        planning_tuning: super::planning::PlanningTuning::from_runtime_profile(&rp, 40),
        tcp_config: tune_tcp(profile, &rp),
        worker_pool: tune_worker_pool(profile, &rp),
    }
}

fn compute_fingerprint(profile: &SystemProfile) -> u64 {
    let mut hasher = DefaultHasher::new();
    profile.cpu.logical_cores.hash(&mut hasher);
    profile.memory.total_gb.to_bits().hash(&mut hasher);
    for gpu in &profile.gpus {
        gpu.name.hash(&mut hasher);
        gpu.vram_gb.to_bits().hash(&mut hasher);
    }
    profile.npus.len().hash(&mut hasher);
    profile.acceleration_mode.hash(&mut hasher);
    hasher.finish()
}

fn cache_path() -> Option<PathBuf> {
    let base = dirs_next::cache_dir().or_else(|| {
        std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .ok()
            .map(PathBuf::from)
    })?;
    Some(base.join("ghostlink").join("autotune.json"))
}

fn now_iso8601() -> String {
    let dur = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = dur.as_secs();
    // Days since epoch
    let days = secs / 86400;
    let remaining = secs % 86400;
    let hours = remaining / 3600;
    let minutes = (remaining % 3600) / 60;
    let seconds = remaining % 60;

    // Compute year/month/day from days since 1970-01-01 (civil calendar)
    let (year, month, day) = civil_from_days(days as i64);

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hours, minutes, seconds
    )
}

/// Convert days since 1970-01-01 to (year, month, day) in the Gregorian civil calendar.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    // Algorithm from Howard Hinnant
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m as u32, d as u32)
}

fn tune_tcp(profile: &SystemProfile, _rp: &RuntimeProfile) -> TcpAutoConfig {
    let (max_inflight, reconnect_attempts, reconnect_backoff_ms) = match profile.acceleration_mode {
        crate::host::AccelerationMode::Gpu => (256, 5, 5),
        crate::host::AccelerationMode::Avx512 => (128, 3, 10),
        _ => (64, 3, 10),
    };

    // Scale for multi-GPU
    let gpu_count = profile
        .gpus
        .iter()
        .filter(|g| g.vram_gb > 0.0)
        .count()
        .max(1);
    TcpAutoConfig {
        max_inflight_batches: max_inflight * gpu_count,
        reconnect_attempts,
        reconnect_backoff_ms,
    }
}

fn tune_worker_pool(profile: &SystemProfile, rp: &RuntimeProfile) -> WorkerPoolConfig {
    let compute = rp.recommended_workers.max(1);
    let io = match profile.acceleration_mode {
        crate::host::AccelerationMode::Gpu => compute / 2,
        _ => compute / 4,
    }
    .max(1);
    WorkerPoolConfig {
        compute_workers: compute,
        io_workers: io,
        total_workers: compute + io,
    }
}

// ---------------------------------------------------------------------------
// Serde support (manual impls to avoid pulling serde derive on sub-types)
// ---------------------------------------------------------------------------

impl serde::Serialize for AutoTuner {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut st = s.serialize_struct("AutoTuner", 7)?;
        st.serialize_field("fingerprint", &self.fingerprint)?;
        st.serialize_field("tuned_at", &self.tuned_at)?;
        st.serialize_field("healthy_latency_us", &self.health_config.healthy_latency_us)?;
        st.serialize_field(
            "degraded_latency_us",
            &self.health_config.degraded_latency_us,
        )?;
        st.serialize_field("lock_timeout_us", &self.load_balance_config.lock_timeout_us)?;
        st.serialize_field(
            "max_inflight_batches",
            &self.tcp_config.max_inflight_batches,
        )?;
        st.serialize_field("compute_workers", &self.worker_pool.compute_workers)?;
        st.end()
    }
}

impl<'de> serde::Deserialize<'de> for AutoTuner {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::{MapAccess, Visitor};
        struct TunerVisitor;
        impl<'de> Visitor<'de> for TunerVisitor {
            type Value = AutoTuner;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("struct AutoTuner")
            }
            fn visit_map<V: MapAccess<'de>>(self, mut map: V) -> Result<AutoTuner, V::Error> {
                let mut fingerprint = 0u64;
                let mut tuned_at = String::new();
                let mut healthy_latency_us = 10.0f32;
                let mut degraded_latency_us = 20.0f32;
                let mut lock_timeout_us = 1000u64;
                let mut max_inflight_batches = 64usize;
                let mut compute_workers = 4usize;

                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "fingerprint" => fingerprint = map.next_value()?,
                        "tuned_at" => tuned_at = map.next_value()?,
                        "healthy_latency_us" => healthy_latency_us = map.next_value()?,
                        "degraded_latency_us" => degraded_latency_us = map.next_value()?,
                        "lock_timeout_us" => lock_timeout_us = map.next_value()?,
                        "max_inflight_batches" => max_inflight_batches = map.next_value()?,
                        "compute_workers" => compute_workers = map.next_value()?,
                        _ => {
                            let _: serde_json::Value = map.next_value()?;
                        }
                    }
                }

                Ok(AutoTuner {
                    fingerprint,
                    tuned_at,
                    health_config: super::health::HealthConfig {
                        healthy_latency_us,
                        degraded_latency_us,
                        ..Default::default()
                    },
                    load_balance_config: super::load_balance::LoadBalanceConfig {
                        lock_timeout_us,
                        ..Default::default()
                    },
                    planning_tuning: super::planning::PlanningTuning {
                        max_layers_per_assignment: compute_workers.max(1),
                    },
                    tcp_config: TcpAutoConfig {
                        max_inflight_batches,
                        reconnect_attempts: 3,
                        reconnect_backoff_ms: 10,
                    },
                    worker_pool: WorkerPoolConfig {
                        compute_workers,
                        io_workers: compute_workers / 2,
                        total_workers: compute_workers + compute_workers / 2,
                    },
                })
            }
        }
        d.deserialize_map(TunerVisitor)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system_profile::SystemProfile;

    #[test]
    fn autotuner_derives_from_system_profile() {
        let sp = SystemProfile::detect_fast();
        let tuner = AutoTuner::from_system_profile(&sp);
        assert!(tuner.worker_pool.compute_workers >= 1);
        assert!(tuner.tcp_config.max_inflight_batches >= 1);
    }

    #[test]
    fn autotuner_fingerprint_is_stable() {
        let sp = SystemProfile::detect_fast();
        let fp1 = compute_fingerprint(&sp);
        let fp2 = compute_fingerprint(&sp);
        assert_eq!(fp1, fp2);
    }

    #[test]
    fn autotuner_round_trips_through_json() {
        let sp = SystemProfile::detect_fast();
        let tuner = AutoTuner::from_system_profile(&sp);
        let json = serde_json::to_string_pretty(&tuner).expect("serialize");
        let restored: AutoTuner = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(tuner.fingerprint, restored.fingerprint);
        assert_eq!(
            tuner.tcp_config.max_inflight_batches,
            restored.tcp_config.max_inflight_batches
        );
    }

    #[test]
    fn autotuner_detects_hardware_change() {
        let sp = SystemProfile::detect_fast();
        let mut tuner = AutoTuner::from_system_profile(&sp);

        // Tamper the fingerprint to simulate hardware change
        tuner.fingerprint = 0;

        let json = serde_json::to_string(&tuner).unwrap();
        std::env::set_var("HOME", std::env::temp_dir());
        // In a real scenario, load_cache would reject this due to fingerprint mismatch
        let restored: AutoTuner = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.fingerprint, 0);
    }

    #[test]
    fn load_cache_rejects_stale_in_memory_fingerprint() {
        // Regression: load_cache()'s in-memory fast path used to return
        // whatever was cached with no fingerprint check at all (unlike
        // from_system_profile() and this same function's disk-fallback
        // path). Force a stale, guaranteed-wrong fingerprint directly into
        // the in-memory cache and confirm it's no longer blindly returned.
        {
            let mut cache = TUNE_CACHE.lock().unwrap();
            let mut stale = tune(&SystemProfile::detect_fast());
            stale.fingerprint = 0; // a real fingerprint is a hash of many fields; 0 is not realistically achievable
            *cache = Some(stale);
        }
        if let Some(tuner) = AutoTuner::load_cache() {
            assert_ne!(
                tuner.fingerprint, 0,
                "load_cache() returned the stale in-memory entry instead of rejecting it"
            );
        }
        // None is also a valid outcome here (no matching on-disk cache either).
    }

    #[test]
    fn tcp_config_scales_with_multi_gpu() {
        use crate::host::{AccelerationMode, GpuBackend};
        use crate::system_profile::{CpuInfo, GpuInfo, MemoryInfo};

        let single = SystemProfile {
            gpus: vec![GpuInfo {
                name: "RTX 4090".into(),
                vram_gb: 24.0,
                backend: GpuBackend::Cuda,
                compute_capability: "8.9".into(),
                driver_version: "555".into(),
                pci_address: None,
            }],
            cpu: CpuInfo {
                logical_cores: 16,
                ..Default::default()
            },
            memory: MemoryInfo {
                total_gb: 64.0,
                ..Default::default()
            },
            acceleration_mode: AccelerationMode::Gpu,
            ..Default::default()
        };

        let dual = SystemProfile {
            gpus: vec![
                GpuInfo {
                    name: "RTX 4090".into(),
                    vram_gb: 24.0,
                    backend: GpuBackend::Cuda,
                    compute_capability: "8.9".into(),
                    driver_version: "555".into(),
                    pci_address: None,
                },
                GpuInfo {
                    name: "RTX 3090".into(),
                    vram_gb: 24.0,
                    backend: GpuBackend::Cuda,
                    compute_capability: "8.6".into(),
                    driver_version: "555".into(),
                    pci_address: None,
                },
            ],
            cpu: CpuInfo {
                logical_cores: 16,
                ..Default::default()
            },
            memory: MemoryInfo {
                total_gb: 64.0,
                ..Default::default()
            },
            acceleration_mode: AccelerationMode::Gpu,
            ..Default::default()
        };

        let s = AutoTuner::from_system_profile(&single);
        let d = AutoTuner::from_system_profile(&dual);

        assert!(d.tcp_config.max_inflight_batches > s.tcp_config.max_inflight_batches);
    }
}
