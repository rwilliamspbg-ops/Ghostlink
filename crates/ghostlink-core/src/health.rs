//! Network Health Monitoring for Ghost-Link Cluster
//!
//! This module provides:
//! - Ping/pong latency tracking per node
//! - Delivery ratio monitoring
//! - Automatic quantization fallback triggers
//! - Fault detection and recovery

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::cluster::{ClusterState, NodeStatus};

use crate::host::{AccelerationMode, RuntimeProfile};

/// Health check configuration
#[derive(Clone, Copy, Debug)]
pub struct HealthConfig {
    /// Interval between health checks
    pub check_interval: Duration,
    /// Timeout for health check response
    pub timeout: Duration,
    /// Minimum number of successful checks before considering node healthy
    pub min_successes: usize,
    /// Maximum allowed failures before marking node degraded
    pub max_failures: usize,
    /// Upper latency bound for a healthy node.
    pub healthy_latency_us: f32,
    /// Upper latency bound for a degraded-but-usable node.
    pub degraded_latency_us: f32,
    /// Minimum delivery ratio for a healthy node.
    pub healthy_delivery_ratio: f32,
    /// Minimum delivery ratio for a degraded node.
    pub degraded_delivery_ratio: f32,
}

impl Default for HealthConfig {
    fn default() -> Self {
        Self {
            check_interval: Duration::from_secs(5),
            timeout: Duration::from_secs(3),
            min_successes: 2,
            max_failures: 3,
            healthy_latency_us: 10.0,
            degraded_latency_us: 20.0,
            healthy_delivery_ratio: 0.95,
            degraded_delivery_ratio: 0.80,
        }
    }
}

impl HealthConfig {
    /// Tune health thresholds from the detected runtime profile.
    pub fn autotuned(profile: &RuntimeProfile) -> Self {
        let (healthy_latency_us, degraded_latency_us, timeout, max_failures) =
            match profile.acceleration_mode {
                AccelerationMode::Gpu => (6.0, 14.0, Duration::from_secs(2), 4),
                AccelerationMode::Avx512 => (8.0, 18.0, Duration::from_secs(2), 3),
                _ => (10.0, 20.0, Duration::from_secs(3), 3),
            };

        Self {
            check_interval: Duration::from_secs(5),
            timeout,
            min_successes: if profile.recommended_workers >= 8 {
                3
            } else {
                2
            },
            max_failures,
            healthy_latency_us,
            degraded_latency_us,
            healthy_delivery_ratio: if profile.node_resources.vram_gb > 0.0 {
                0.97
            } else {
                0.95
            },
            degraded_delivery_ratio: if profile.node_resources.vram_gb > 0.0 {
                0.88
            } else {
                0.80
            },
        }
    }
}

/// Health status for a node
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HealthStatus {
    /// Node is healthy
    Healthy,
    /// Node is degraded (performance issues)
    Degraded,
    /// Node has failed
    Failed,
    /// No health data available yet
    #[default]
    Unknown,
}

/// Health check result
#[derive(Clone, Debug)]
pub struct HealthCheckResult {
    /// Node ID
    pub node_id: String,
    /// Latency in microseconds
    pub latency_us: f32,
    /// Delivery ratio (0.0 to 1.0)
    pub delivery_ratio: f32,
    /// Status
    pub status: HealthStatus,
    /// Timestamp of check
    pub timestamp: Instant,
}

/// Network health monitor
#[derive(Clone, Debug)]
pub struct NetworkHealthMonitor {
    /// Cluster state
    cluster: Arc<ClusterState>,
    /// Configuration
    config: HealthConfig,
    /// Last check timestamp
    last_check: Arc<Mutex<Option<Instant>>>,
    /// Recent check results per node
    recent_checks: Arc<Mutex<HashMap<String, Vec<HealthCheckResult>>>>,
}

impl NetworkHealthMonitor {
    /// Create new health monitor
    pub fn new(cluster: Arc<ClusterState>, config: HealthConfig) -> Self {
        Self {
            cluster,
            config,
            last_check: Arc::new(Mutex::new(None)),
            recent_checks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Create a monitor with thresholds derived from runtime auto-detection.
    pub fn with_runtime_profile(cluster: Arc<ClusterState>, profile: &RuntimeProfile) -> Self {
        Self::new(cluster, HealthConfig::autotuned(profile))
    }

    /// Process incoming health check frame from network
    ///
    /// Updates node metrics and health status based on received heartbeat data.
    pub fn process_health_frame(&self, frame: &crate::protocol::HealthCheckFrame) {
        let delivery_ratio = frame.delivery_ratio as f32 / 100.0;

        self.cluster.get_metrics_mut(&frame.node_id, |metrics| {
            metrics.record_latency(frame.latency_us as f32);
            metrics.record_delivery_ratio(delivery_ratio);
        });

        // Store the health check result for tracking
        let now = Instant::now();
        let result = HealthCheckResult {
            node_id: frame.node_id.clone(),
            latency_us: frame.latency_us as f32,
            delivery_ratio,
            status: self.get_health_status(frame.latency_us as f32, delivery_ratio),
            timestamp: now,
        };

        let mut checks = self.recent_checks.lock().unwrap();
        if let Some(node_checks) = checks.get_mut(&frame.node_id) {
            node_checks.push(result.clone());
            if node_checks.len() > 10 {
                node_checks.remove(0);
            }
        } else {
            checks.insert(frame.node_id.clone(), vec![result]);
        }
    }

    /// Run health check on all nodes
    pub fn check_all(&self) {
        let now = Instant::now();

        for node in self.cluster.nodes_snapshot().iter() {
            // Simulate health check inputs (production would gather real probe results)
            let latency_us = 1.0 + (rand::random::<f32>() * 0.5);
            let delivery_ratio = 0.98 - (rand::random::<f32>() * 0.05);

            if self
                .cluster
                .get_metrics_mut(&node.id, |m| {
                    m.record_latency(latency_us);
                    m.record_delivery_ratio(delivery_ratio);
                })
                .is_some()
            {
                let result = HealthCheckResult {
                    node_id: node.id.clone(),
                    latency_us,
                    delivery_ratio,
                    status: self.get_health_status(latency_us, delivery_ratio),
                    timestamp: now,
                };

                // Store recent check results (keep last 10)
                let mut checks = self.recent_checks.lock().unwrap();
                if let Some(node_checks) = checks.get_mut(&node.id) {
                    node_checks.push(result.clone());
                    if node_checks.len() > 10 {
                        node_checks.remove(0);
                    }
                } else {
                    checks.insert(node.id.clone(), vec![result]);
                }
            }
        }

        *self.last_check.lock().unwrap() = Some(now);
    }

    /// Get health status based on metrics
    fn get_health_status(&self, latency_us: f32, delivery_ratio: f32) -> HealthStatus {
        if delivery_ratio >= self.config.healthy_delivery_ratio
            && latency_us <= self.config.healthy_latency_us
        {
            HealthStatus::Healthy
        } else if delivery_ratio >= self.config.degraded_delivery_ratio
            || latency_us <= self.config.degraded_latency_us
        {
            HealthStatus::Degraded
        } else {
            HealthStatus::Failed
        }
    }

    /// Get health report for a specific node
    pub fn get_node_health(&self, node_id: &str) -> Option<HealthCheckResult> {
        let checks = self.recent_checks.lock().unwrap();
        checks
            .get(node_id)
            .and_then(|checks| checks.last())
            .cloned()
    }

    /// Get health report for all nodes
    pub fn get_all_health(&self) -> Vec<HealthCheckResult> {
        let checks = self.recent_checks.lock().unwrap();
        checks.values().flat_map(|checks| checks.clone()).collect()
    }

    /// Check if node needs quantization fallback
    pub fn needs_quantization_fallback(&self, node_id: &str) -> bool {
        let checks = self.recent_checks.lock().unwrap();

        if let Some(node_checks) = checks.get(node_id) {
            // Check last 3 results for consistent degradation
            let recent: Vec<_> = node_checks.iter().rev().take(3).collect();

            if recent.len() < 3 {
                return false;
            }

            // Calculate average delivery ratio of last 3 checks
            let avg_delivery_ratio: f32 =
                recent.iter().map(|r| r.delivery_ratio).sum::<f32>() / 3.0;

            // Check average latency
            let avg_latency_us: f32 = recent.iter().map(|r| r.latency_us).sum::<f32>() / 3.0;

            // Need fallback if delivery ratio dropped below threshold or latency increased significantly
            avg_delivery_ratio < self.config.healthy_delivery_ratio
                || avg_latency_us > self.config.healthy_latency_us
        } else {
            false
        }
    }

    /// Get cluster-wide health summary
    pub fn get_health_summary(&self) -> String {
        let checks = self.recent_checks.lock().unwrap();

        let total_nodes = checks.len();
        let healthy_count = checks
            .values()
            .filter(|node_checks| {
                node_checks
                    .last()
                    .map(|c| c.status == HealthStatus::Healthy)
                    .unwrap_or(false)
            })
            .count();

        let degraded_count = checks
            .values()
            .filter(|node_checks| {
                node_checks
                    .last()
                    .map(|c| c.status == HealthStatus::Degraded)
                    .unwrap_or(false)
            })
            .count();

        let failed_count = checks
            .values()
            .filter(|node_checks| {
                node_checks
                    .last()
                    .map(|c| c.status == HealthStatus::Failed)
                    .unwrap_or(false)
            })
            .count();

        format!(
            "Network Health Summary\n\
             =================\n\
             Total nodes: {}\n\
             Healthy: {}\n\
             Degraded: {}\n\
             Failed: {}\n",
            total_nodes, healthy_count, degraded_count, failed_count
        )
    }

    /// Start periodic health checks in background
    pub fn start_periodic_checks(&self) {
        let this = self.clone();

        std::thread::spawn(move || loop {
            this.check_all();
            std::thread::sleep(this.config.check_interval);
        });
    }
}

/// Health metrics aggregation
#[derive(Clone, Debug, Default)]
pub struct HealthMetrics {
    /// Average latency across cluster
    pub avg_latency_us: f32,
    /// Minimum observed latency
    pub min_latency_us: f32,
    /// Maximum observed latency
    pub max_latency_us: f32,
    /// Average delivery ratio
    pub avg_delivery_ratio: f32,
    /// Minimum delivery ratio
    pub min_delivery_ratio: f32,
    /// Number of samples
    pub sample_count: usize,
}

impl HealthMetrics {
    /// Create new metrics aggregator
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a health check result
    pub fn record(&mut self, latency_us: f32, delivery_ratio: f32) {
        if self.sample_count == 0 {
            self.min_latency_us = latency_us;
            self.max_latency_us = latency_us;
            self.min_delivery_ratio = delivery_ratio;
        } else {
            if latency_us < self.min_latency_us {
                self.min_latency_us = latency_us;
            }
            if latency_us > self.max_latency_us {
                self.max_latency_us = latency_us;
            }
            if delivery_ratio < self.min_delivery_ratio {
                self.min_delivery_ratio = delivery_ratio;
            }
        }

        // Running mean
        let n = self.sample_count as f32;
        self.avg_latency_us = (self.avg_latency_us * n + latency_us) / (n + 1.0);
        self.avg_delivery_ratio = (self.avg_delivery_ratio * n + delivery_ratio) / (n + 1.0);

        self.sample_count += 1;
    }

    /// Get health status recommendation
    pub fn get_recommendation(&self) -> String {
        if self.sample_count == 0 {
            return "No data available".to_string();
        }

        match (self.avg_delivery_ratio, self.min_latency_us) {
            (ratio, latency) if ratio >= 0.95 && latency <= 10.0 => {
                "Cluster is healthy. No action needed.".to_string()
            }
            (ratio, latency) if ratio >= 0.90 && latency <= 20.0 => {
                "Cluster is degraded. Consider load balancing.".to_string()
            }
            (ratio, latency) if ratio >= 0.80 && latency <= 50.0 => {
                "Cluster performance is poor. Recommend quantization fallback.".to_string()
            }
            _ => "Cluster has failed nodes. Investigate and recover.".to_string(),
        }
    }
}

/// Fault detection and recovery system
#[derive(Clone, Debug)]
pub struct FaultDetector {
    /// Cluster state
    cluster: Arc<ClusterState>,
    /// Detection interval
    detection_interval: Duration,
}

impl FaultDetector {
    /// Create new fault detector
    pub fn new(cluster: Arc<ClusterState>, detection_interval: Duration) -> Self {
        Self {
            cluster,
            detection_interval,
        }
    }

    /// Detect failed nodes based on heartbeat timeouts
    pub fn detect_failures(&self) -> Vec<String> {
        let mut failed_nodes: Vec<String> = Vec::new();

        for node in self.cluster.nodes_snapshot().iter() {
            if self.cluster.check_heartbeat_timeout(&node.id) {
                if let Some(metrics) = self.cluster.get_metrics(&node.id) {
                    // Check if node is already marked as failed
                    if metrics.status == NodeStatus::Active {
                        self.cluster.mark_failed(&node.id);
                        failed_nodes.push(node.id.clone());
                    }
                } else {
                    // No metrics for this node - mark as failed
                    self.cluster.mark_failed(&node.id);
                    failed_nodes.push(node.id.clone());
                }
            }
        }

        failed_nodes
    }

    /// Recover a failed node
    pub fn recover_node(&self, node_id: &str) {
        self.cluster.get_metrics_mut(node_id, |metrics| {
            metrics.status = NodeStatus::Active;
            metrics.last_heartbeat = Instant::now();

            // Reset metrics
            metrics.avg_latency_us = 0.0;
            metrics.min_latency_us = f32::MAX;
            metrics.max_latency_us = 0.0;
            metrics.latency_samples = 0;
            metrics.delivery_ratio = 1.0;
            metrics.throughput_gbps = 0.0;
        });
    }

    /// Start periodic fault detection in background
    pub fn start_periodic_detection(&self) {
        let this = self.clone();

        std::thread::spawn(move || loop {
            let failed = this.detect_failures();
            if !failed.is_empty() {
                tracing::warn!(
                    "Detected {} failed nodes: {:?}",
                    failed.len(),
                    failed.iter().map(|s| s.as_str()).collect::<Vec<_>>()
                );
            }
            std::thread::sleep(this.detection_interval);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cluster::ClusterState;
    use crate::protocol::NodeResources;
    use std::sync::Arc;
    use std::time::Duration;

    #[test]
    fn health_monitor_checks_all_nodes() {
        let cluster = Arc::new(ClusterState::new());
        cluster.register(NodeResources::new("node-a", 24.0, 64.0, "8.9", None));
        cluster.register(NodeResources::new("node-b", 12.0, 32.0, "8.6", None));

        let monitor = NetworkHealthMonitor::new(cluster.clone(), HealthConfig::default());
        monitor.check_all();

        let summary = monitor.get_health_summary();
        assert!(summary.contains("Total nodes: 2"));
    }

    #[test]
    fn health_monitor_detects_degraded_nodes() {
        let cluster = Arc::new(ClusterState::new());
        cluster.register(NodeResources::new("node-a", 24.0, 64.0, "8.9", None));

        let monitor = NetworkHealthMonitor::new(cluster.clone(), HealthConfig::default());

        // Simulate multiple degraded health checks by seeding recent_checks directly
        // (check_all uses random healthy values, so we manually push degraded results)
        for _ in 0..3 {
            let result = HealthCheckResult {
                node_id: "node-a".to_string(),
                latency_us: 15.0,
                delivery_ratio: 0.85,
                status: HealthStatus::Degraded,
                timestamp: std::time::Instant::now(),
            };
            monitor
                .recent_checks
                .lock()
                .unwrap()
                .entry("node-a".to_string())
                .or_default()
                .push(result);
        }

        assert!(monitor.needs_quantization_fallback("node-a"));
    }

    #[test]
    fn health_metrics_aggregates() {
        let mut metrics = HealthMetrics::new();

        metrics.record(1.0, 0.98);
        metrics.record(2.0, 0.95);
        metrics.record(1.5, 0.97);

        assert!((metrics.avg_latency_us - 1.5).abs() < 0.1);
        assert!(metrics.min_latency_us <= 1.0);
        assert!(metrics.max_latency_us >= 2.0);
    }

    #[test]
    fn fault_detector_detects_failures() {
        let cluster = Arc::new(ClusterState::new());
        cluster.register(NodeResources::new("node-a", 24.0, 64.0, "8.9", None));

        // Wait for timeout
        std::thread::sleep(Duration::from_secs(6));

        let detector = FaultDetector::new(cluster.clone(), Duration::from_secs(1));
        let failed = detector.detect_failures();

        assert!(failed.contains(&"node-a".to_string()));
    }

    #[test]
    fn fault_detector_recovers_nodes() {
        let cluster = Arc::new(ClusterState::new());
        cluster.register(NodeResources::new("node-a", 24.0, 64.0, "8.9", None));

        std::thread::sleep(Duration::from_secs(6));

        let detector = FaultDetector::new(cluster.clone(), Duration::from_secs(1));
        detector.detect_failures();

        assert!(cluster.get_metrics("node-a").unwrap().status == NodeStatus::Failed);

        detector.recover_node("node-a");
        assert_eq!(
            cluster.get_metrics("node-a").unwrap().status,
            NodeStatus::Active
        );
    }

    #[test]
    fn health_monitor_gets_recommendation() {
        let mut metrics = HealthMetrics::new();

        // Good cluster
        metrics.record(1.0, 0.98);
        metrics.record(2.0, 0.95);

        let recommendation = metrics.get_recommendation();
        assert!(recommendation.contains("healthy"));
    }

    #[test]
    fn health_config_autotunes_for_gpu_profiles() {
        let profile = RuntimeProfile {
            node_resources: NodeResources::new("node-a", 24.0, 64.0, "8.9", None),
            logical_cores: 16,
            recommended_workers: 8,
            acceleration_mode: AccelerationMode::Gpu,
            xdp_supported: true,
            detection_source: String::from("test"),
            probe_mode: crate::host::ProbeMode::Fast,
        };

        let config = HealthConfig::autotuned(&profile);
        assert!(config.healthy_latency_us < 10.0);
        assert!(config.healthy_delivery_ratio > 0.95);
    }
}

#[cfg(test)]
mod tests_health_frame_processing {
    use super::*;
    use crate::protocol::HealthCheckFrame;
    use crate::protocol::NodeResources;
    use std::sync::Arc;

    #[test]
    fn test_process_health_frame_updates_metrics() {
        let cluster = Arc::new(ClusterState::new());
        cluster.register(NodeResources::new("node-01", 24.0, 64.0, "8.9", None));

        let monitor = NetworkHealthMonitor::new(cluster.clone(), HealthConfig::default());

        // Process a health check frame
        let frame = HealthCheckFrame::new("node-01", 1500, 0.95);
        monitor.process_health_frame(&frame);

        // Verify metrics were updated
        let metrics = cluster.get_metrics("node-01").unwrap();
        assert!((metrics.avg_latency_us - 1500.0).abs() < 1.0);
        assert!(metrics.delivery_ratio > 0.9);
    }

    #[test]
    fn test_multi_node_auto_discovery_scenario() {
        let cluster = Arc::new(ClusterState::new());

        // Simulate discovering 3 nodes
        cluster.register(NodeResources::new(
            "node-01",
            24.0,
            64.0,
            "8.9",
            Some("RTX4090".into()),
        ));
        cluster.register(NodeResources::new(
            "node-02",
            12.0,
            32.0,
            "8.6",
            Some("RTX3080".into()),
        ));
        cluster.register(NodeResources::new(
            "node-03",
            48.0,
            128.0,
            "9.0",
            Some("RTX6000".into()),
        ));

        let monitor = NetworkHealthMonitor::new(cluster.clone(), HealthConfig::default());

        // Simulate health checks from each node
        monitor.process_health_frame(&HealthCheckFrame::new("node-01", 1200, 0.99));
        monitor.process_health_frame(&HealthCheckFrame::new("node-02", 8000, 0.85));
        monitor.process_health_frame(&HealthCheckFrame::new("node-03", 900, 0.98));

        // All nodes discoverable and health-tracked
        assert!(monitor.get_node_health("node-01").is_some());
        assert!(monitor.get_node_health("node-02").is_some());
        assert!(monitor.get_node_health("node-03").is_some());

        // Verify node-02 is degraded (high latency, low delivery ratio)
        let node2_health = monitor.get_node_health("node-02").unwrap();
        assert_eq!(node2_health.status, HealthStatus::Degraded);
    }
}
