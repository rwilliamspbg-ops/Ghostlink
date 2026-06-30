//! Distributed inference runtime scaffolding.
//!
//! This module provides lightweight execution planning primitives that bridge
//! placement plans to token-step pipeline schedules across heterogeneous devices.

use crate::planning::LayerAssignment;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::io::{self, Read, Write};
use std::io::{BufReader, BufWriter};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::slice;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use std::time::Instant;

/// Execution target selected for a pipeline stage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceKind {
    Npu,
    Gpu,
    Cpu,
}

impl DeviceKind {
    /// Human-friendly label.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Npu => "NPU",
            Self::Gpu => "GPU",
            Self::Cpu => "CPU",
        }
    }
}

/// Placement of one contiguous layer range onto a node and device.
#[derive(Clone, Debug, PartialEq)]
pub struct StagePlacement {
    pub node_id: String,
    pub start_layer: usize,
    pub end_layer: usize,
    pub device: DeviceKind,
    /// Estimated compute latency for one micro-batch token step.
    pub est_latency_ms: f32,
}

impl StagePlacement {
    pub fn num_layers(&self) -> usize {
        self.end_layer.saturating_sub(self.start_layer)
    }
}

/// Pipeline execution plan for one model.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PipelinePlan {
    pub stages: Vec<StagePlacement>,
}

impl PipelinePlan {
    /// Build a pipeline plan from layer assignments and a node/device map.
    pub fn from_assignments(
        assignments: &[LayerAssignment],
        device_by_node: &std::collections::HashMap<String, DeviceKind>,
    ) -> Self {
        Self::from_assignments_with_measured(assignments, device_by_node, &Default::default())
    }

    /// Build a pipeline plan using measured latencies from cluster metrics if available.
    pub fn from_assignments_with_measured(
        assignments: &[LayerAssignment],
        device_by_node: &std::collections::HashMap<String, DeviceKind>,
        cluster: &crate::cluster::ClusterState,
    ) -> Self {
        let stages = assignments
            .iter()
            .map(|assignment| {
                let device = device_by_node
                    .get(&assignment.node_id)
                    .copied()
                    .unwrap_or(DeviceKind::Cpu);

                // Use measured latency if available (converted from us to ms per layer)
                let per_layer_cost_ms =
                    if let Some(metrics) = cluster.get_metrics(&assignment.node_id) {
                        if metrics.latency_samples > 0 && assignment.num_layers > 0 {
                            (metrics.avg_latency_us / 1000.0) / assignment.num_layers as f32
                        } else {
                            Self::default_cost_for_device(device)
                        }
                    } else {
                        Self::default_cost_for_device(device)
                    };

                StagePlacement {
                    node_id: assignment.node_id.clone(),
                    start_layer: assignment.start_layer,
                    end_layer: assignment.end_layer,
                    device,
                    est_latency_ms: per_layer_cost_ms * assignment.num_layers as f32,
                }
            })
            .collect();

        Self { stages }
    }

    fn default_cost_for_device(device: DeviceKind) -> f32 {
        match device {
            DeviceKind::Npu => 0.42,
            DeviceKind::Gpu => 0.55,
            DeviceKind::Cpu => 1.25,
        }
    }

    /// Return aggregate token-step latency estimate for one micro-batch.
    pub fn est_step_latency_ms(&self) -> f32 {
        self.stages.iter().map(|s| s.est_latency_ms).sum()
    }

    pub fn summary(&self) -> String {
        let mut out = String::from("Pipeline Plan\n");
        out.push_str("============\n");

        for (idx, stage) in self.stages.iter().enumerate() {
            out.push_str(&format!(
                "Stage {}: {} layers {}-{} on {} ({:.2} ms)\n",
                idx,
                stage.node_id,
                stage.start_layer,
                stage.end_layer,
                stage.device.as_str(),
                stage.est_latency_ms
            ));
        }

        out.push_str(&format!(
            "Estimated per-token step latency: {:.2} ms\n",
            self.est_step_latency_ms()
        ));
        out
    }
}

fn run_stage_compute(payload: &mut [f32], stage: &StagePlacement) {
    let base_rounds = stage.num_layers().max(1) / 4 + 1;
    let rounds = match stage.device {
        DeviceKind::Npu => base_rounds,
        DeviceKind::Gpu => base_rounds * 2,
        DeviceKind::Cpu => base_rounds * 3,
    };

    let alpha = match stage.device {
        DeviceKind::Npu => 1.001_f32,
        DeviceKind::Gpu => 1.003_f32,
        DeviceKind::Cpu => 1.005_f32,
    };

    for _ in 0..rounds {
        for value in payload.iter_mut() {
            *value = *value * alpha + 0.125;
        }
    }
}

/// Per-stage runtime telemetry captured from real in-process execution.
#[derive(Clone, Debug, PartialEq)]
pub struct StageExecutionStats {
    pub stage_idx: usize,
    pub processed_batches: usize,
    pub avg_compute_ms: f32,
    pub avg_recv_wait_ms: f32,
    pub avg_send_wait_ms: f32,
    pub avg_bridge_write_ms: f32,
    pub avg_bridge_read_ms: f32,
}

/// Aggregate runtime execution output.
#[derive(Clone, Debug, PartialEq)]
pub struct ExecutionResult {
    pub token_count: usize,
    pub micro_batch: usize,
    pub batch_count: usize,
    pub stage_count: usize,
    pub total_time_ms: f32,
    pub throughput_tokens_per_sec: f32,
    pub avg_token_latency_ms: f32,
    pub p95_token_latency_ms: f32,
    pub stage_stats: Vec<StageExecutionStats>,
}

/// Runtime controls for TCP transport execution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TcpTransportConfig {
    pub max_inflight_batches: usize,
    pub reconnect_attempts: usize,
    pub reconnect_backoff_ms: u64,
    pub auth_token: Option<String>,
    pub use_mtls: bool,
    pub cert_chain_path: Option<String>,
}

impl Default for TcpTransportConfig {
    fn default() -> Self {
        Self {
            max_inflight_batches: 512,
            reconnect_attempts: 3,
            reconnect_backoff_ms: 25,
            auth_token: None,
            use_mtls: false,
            cert_chain_path: None,
        }
    }
}

impl ExecutionResult {
    pub fn summary(&self) -> String {
        let mut out = String::from("Execution Runtime\n");
        out.push_str("=================\n");
        out.push_str(&format!(
            "Tokens: {} | Micro-batch: {} | Batches: {} | Stages: {}\n",
            self.token_count, self.micro_batch, self.batch_count, self.stage_count
        ));
        out.push_str(&format!(
            "Measured wall-clock time: {:.2} ms\n",
            self.total_time_ms
        ));
        out.push_str(&format!(
            "Throughput: {:.2} tokens/sec\n",
            self.throughput_tokens_per_sec
        ));
        out.push_str(&format!(
            "Avg token latency: {:.2} ms | P95: {:.2} ms\n",
            self.avg_token_latency_ms, self.p95_token_latency_ms
        ));

        for stage in &self.stage_stats {
            out.push_str(&format!(
                "Stage {} batches={} compute={:.2} ms recv-wait={:.2} ms send-wait={:.2} ms bridge-write={:.2} ms bridge-read={:.2} ms\n",
                stage.stage_idx,
                stage.processed_batches,
                stage.avg_compute_ms,
                stage.avg_recv_wait_ms,
                stage.avg_send_wait_ms,
                stage.avg_bridge_write_ms,
                stage.avg_bridge_read_ms
            ));
        }

        out
    }
}

/// One scheduled operation in steady-state token generation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TokenStep {
    pub token_idx: usize,
    pub stage_idx: usize,
}

/// Generate a simple steady-state pipeline schedule preview.
///
/// For each token, all stages execute in order; this preview is useful for
/// verifying stage count and queue depth wiring before integrating transport.
pub fn build_token_schedule(stage_count: usize, token_count: usize) -> Vec<TokenStep> {
    let mut schedule = Vec::with_capacity(stage_count.saturating_mul(token_count));
    for token_idx in 0..token_count {
        for stage_idx in 0..stage_count {
            schedule.push(TokenStep {
                token_idx,
                stage_idx,
            });
        }
    }
    schedule
}

/// Execute pipeline stages with real channel/thread wiring for `token_count` tokens.
///
/// `micro_batch` controls how many tokens are packaged per batch. Values below
/// 1 are clamped to 1.
pub fn execute_pipeline(
    plan: &PipelinePlan,
    token_count: usize,
    micro_batch: usize,
) -> ExecutionResult {
    execute_pipeline_with_rebalance(plan, token_count, micro_batch, None)
}

pub fn execute_pipeline_with_rebalance(
    plan: &PipelinePlan,
    token_count: usize,
    micro_batch: usize,
    rebalance: Option<&crate::planning::RebalanceTrigger>,
) -> ExecutionResult {
    execute_pipeline_with_rebalance_and_measured(
        plan,
        token_count,
        micro_batch,
        rebalance,
        None,
        None,
    )
}

pub fn execute_pipeline_with_rebalance_and_measured(
    plan: &PipelinePlan,
    token_count: usize,
    micro_batch: usize,
    rebalance: Option<&crate::planning::RebalanceTrigger>,
    cluster: Option<&crate::cluster::ClusterState>,
    placement_context: Option<&crate::planning::PlacementPlan>,
) -> ExecutionResult {
    let stage_count = plan.stages.len();
    let micro_batch = micro_batch.max(1);
    let batch_count = token_count.div_ceil(micro_batch);
    if stage_count == 0 || token_count == 0 {
        return ExecutionResult {
            token_count,
            micro_batch,
            batch_count,
            stage_count,
            total_time_ms: 0.0,
            throughput_tokens_per_sec: 0.0,
            avg_token_latency_ms: 0.0,
            p95_token_latency_ms: 0.0,
            stage_stats: Vec::new(),
        };
    }

    #[derive(Debug)]
    struct BatchWork {
        tokens_in_batch: usize,
        started_at: Instant,
        payload: Vec<f32>,
    }

    #[derive(Debug)]
    struct StageAccumulator {
        stage_idx: usize,
        processed_batches: usize,
        total_compute_ms: f32,
        total_recv_wait_ms: f32,
        total_send_wait_ms: f32,
    }

    use crate::ring::{RingConfig, SpscRingBuffer};
    use std::sync::Arc;

    // Use zero-copy SPSC ring buffers for high-throughput in-memory execution.
    let ring_cfg = RingConfig {
        capacity: 512,
        backpressure_threshold: 400,
    };

    let mut rings = Vec::with_capacity(stage_count + 1);
    for _ in 0..=stage_count {
        rings.push(Arc::new(SpscRingBuffer::<BatchWork>::new(ring_cfg)));
    }

    // Return ring for recycling payload vectors to reduce allocation churn.
    let recycler_ring = Arc::new(SpscRingBuffer::<Vec<f32>>::new(ring_cfg));

    let mut stage_handles = Vec::with_capacity(stage_count);
    for stage_idx in 0..stage_count {
        let stage = plan.stages[stage_idx].clone();
        let rx_ring = Arc::clone(&rings[stage_idx]);
        let tx_ring = Arc::clone(&rings[stage_idx + 1]);

        let handle = thread::spawn(move || {
            let mut processed_batches = 0usize;
            let mut total_compute_ms = 0.0_f32;
            let mut total_recv_wait_ms = 0.0_f32;
            let mut total_send_wait_ms = 0.0_f32;

            loop {
                let recv_start = Instant::now();
                while rx_ring.is_empty() {
                    if Arc::strong_count(&rx_ring) <= 1 {
                        return StageAccumulator {
                            stage_idx,
                            processed_batches,
                            total_compute_ms,
                            total_recv_wait_ms,
                            total_send_wait_ms,
                        };
                    }
                    thread::yield_now();
                }
                let Some(mut batch) = rx_ring.pop() else {
                    continue;
                };
                total_recv_wait_ms += recv_start.elapsed().as_secs_f32() * 1000.0;

                let compute_start = Instant::now();
                run_stage_compute(&mut batch.payload, &stage);
                total_compute_ms += compute_start.elapsed().as_secs_f32() * 1000.0;

                let send_start = Instant::now();
                tx_ring.wait_for_space();
                let _ = tx_ring.push(batch);
                total_send_wait_ms += send_start.elapsed().as_secs_f32() * 1000.0;
                processed_batches += 1;
            }
        });

        stage_handles.push(handle);
    }

    let completion_ring = Arc::clone(&rings[stage_count]);
    let entry_ring = Arc::clone(&rings[0]);
    // Clear rings vector so strong_count can drop once threads finish
    drop(rings);

    let recycler_ring_c = Arc::clone(&recycler_ring);
    let latencies_handle = thread::spawn(move || {
        let mut latencies = Vec::with_capacity(token_count);
        for _ in 0..batch_count {
            completion_ring.wait_for_data();
            if let Some(done_batch) = completion_ring.pop() {
                let batch_latency_ms = done_batch.started_at.elapsed().as_secs_f32() * 1000.0;
                for _ in 0..done_batch.tokens_in_batch {
                    latencies.push(batch_latency_ms);
                }
                recycler_ring_c.wait_for_space();
                let _ = recycler_ring_c.push(done_batch.payload);
            }
        }
        latencies
    });

    let exec_start = Instant::now();
    for batch_idx in 0..batch_count {
        let batch_start_token = batch_idx * micro_batch;
        let tokens_in_batch = (token_count - batch_start_token).min(micro_batch);
        let payload_len = (tokens_in_batch.max(1) * 16).max(32);

        let mut payload = recycler_ring
            .pop()
            .unwrap_or_else(|| Vec::with_capacity(payload_len));
        payload.resize(payload_len, 0.0);
        for (idx, val) in payload.iter_mut().enumerate() {
            *val = (batch_idx as f32 * 0.01) + (idx as f32 * 0.0001);
        }

        // Evaluate dynamic rebalance trigger during execution
        if let (Some(trigger), Some(cluster_state), Some(placement)) =
            (rebalance, cluster, placement_context)
        {
            // Check for rebalance every 25th batch to allow stability between checks
            if batch_idx % 25 == 0 && batch_idx > 0 {
                if let Some(migration) = trigger.evaluate(cluster_state, placement) {
                    tracing::info!(
                        "Dynamic Runtime: Rebalance Triggered! Moving layers {:?} from {} to {}",
                        migration.layers,
                        migration.source_node,
                        migration.target_node
                    );
                    for step in migration.generate_handoff_plan() {
                        tracing::info!("  -> {}", step);
                    }
                }
            }
        }

        entry_ring.wait_for_space();
        let _ = entry_ring.push(BatchWork {
            tokens_in_batch,
            started_at: Instant::now(),
            payload,
        });
    }
    // Drop entry ring to signal completion to stage threads
    drop(entry_ring);

    let token_latencies = latencies_handle.join().unwrap_or_default();
    let total_time_ms = exec_start.elapsed().as_secs_f32() * 1000.0;

    let mut stage_stats = Vec::with_capacity(stage_count);
    for handle in stage_handles {
        if let Ok(stats) = handle.join() {
            let divisor = stats.processed_batches.max(1) as f32;
            let avg_compute_ms = stats.total_compute_ms / divisor;

            // Feedback loop: update cluster state with actual compute latency
            if let Some(cluster_state) = cluster {
                if let Some(stage) = plan.stages.get(stats.stage_idx) {
                    cluster_state.get_metrics_mut(&stage.node_id, |m| {
                        // Convert ms back to us for ClusterState (avg_latency_us)
                        m.record_latency(avg_compute_ms * 1000.0);
                    });
                }
            }

            stage_stats.push(StageExecutionStats {
                stage_idx: stats.stage_idx,
                processed_batches: stats.processed_batches,
                avg_compute_ms,
                avg_recv_wait_ms: stats.total_recv_wait_ms / divisor,
                avg_send_wait_ms: stats.total_send_wait_ms / divisor,
                avg_bridge_write_ms: 0.0,
                avg_bridge_read_ms: 0.0,
            });
        }
    }
    stage_stats.sort_by_key(|s| s.stage_idx);

    let throughput_tokens_per_sec = if total_time_ms > 0.0 {
        token_count as f32 / (total_time_ms / 1000.0)
    } else {
        0.0
    };

    let avg_token_latency_ms = if token_latencies.is_empty() {
        0.0
    } else {
        token_latencies.iter().sum::<f32>() / token_latencies.len() as f32
    };

    let mut sorted = token_latencies.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let p95_idx = ((sorted.len() - 1) as f32 * 0.95).round() as usize;
    let p95_token_latency_ms = sorted.get(p95_idx).copied().unwrap_or(0.0);

    ExecutionResult {
        token_count,
        micro_batch,
        batch_count,
        stage_count,
        total_time_ms,
        throughput_tokens_per_sec,
        avg_token_latency_ms,
        p95_token_latency_ms,
        stage_stats,
    }
}

#[derive(Debug, Clone)]
pub struct TransportBatch {
    pub batch_id: usize,
    pub tokens_in_batch: usize,
    pub payload: Vec<f32>,
}

fn auth_tag(
    source_stage: usize,
    batch_id: usize,
    tokens_in_batch: usize,
    payload: &[f32],
    token: &str,
) -> [u8; 32] {
    let mut mac = Hmac::<Sha256>::new_from_slice(token.as_bytes())
        .expect("HMAC key setup for transport auth failed");
    mac.update(&(source_stage as u32).to_le_bytes());
    mac.update(&(batch_id as u64).to_le_bytes());
    mac.update(&(tokens_in_batch as u32).to_le_bytes());
    mac.update(&(payload.len() as u32).to_le_bytes());
    let payload_bytes = payload_as_le_bytes(payload);
    mac.update(payload_bytes.as_ref());
    mac.finalize().into_bytes().into()
}

fn payload_as_le_bytes(payload: &[f32]) -> std::borrow::Cow<'_, [u8]> {
    if cfg!(target_endian = "little") {
        // SAFETY: f32 is POD; casting [f32] to its contiguous byte representation is valid.
        let bytes = unsafe {
            slice::from_raw_parts(
                payload.as_ptr() as *const u8,
                std::mem::size_of_val(payload),
            )
        };
        std::borrow::Cow::Borrowed(bytes)
    } else {
        let mut bytes = Vec::with_capacity(payload.len() * 4);
        for value in payload {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        std::borrow::Cow::Owned(bytes)
    }
}

fn write_transport_batch(
    writer: &mut impl Write,
    batch: &TransportBatch,
    source_stage: usize,
    token: Option<&str>,
    frame_buf: &mut Vec<u8>,
) -> io::Result<()> {
    let batch_id = batch.batch_id as u64;
    let tokens = batch.tokens_in_batch as u32;
    let payload_len = batch.payload.len() as u32;
    let source_stage_u16 = source_stage as u16;

    // Reuse frame buffer to minimize allocations.
    frame_buf.clear();
    // Header size: source_stage(2) + batch_id(8) + tokens(4) + payload_len(4) + tag_present(1) + [tag(32) if present]
    let header_capacity = if token.is_some() { 2 + 8 + 4 + 4 + 1 + 32 } else { 2 + 8 + 4 + 4 + 1 };
    frame_buf.reserve(header_capacity + batch.payload.len() * 4);
    frame_buf.extend_from_slice(&source_stage_u16.to_le_bytes());
    frame_buf.extend_from_slice(&batch_id.to_le_bytes());
    frame_buf.extend_from_slice(&tokens.to_le_bytes());
    frame_buf.extend_from_slice(&payload_len.to_le_bytes());

    if let Some(t) = token {
        frame_buf.push(1); // Tag present
        let tag = auth_tag(
            source_stage,
            batch.batch_id,
            batch.tokens_in_batch,
            &batch.payload,
            t,
        );
        frame_buf.extend_from_slice(&tag);
    } else {
        frame_buf.push(0); // Tag absent
    }

    let payload_bytes = payload_as_le_bytes(&batch.payload);
    frame_buf.extend_from_slice(payload_bytes.as_ref());
    writer.write_all(frame_buf)?;

    Ok(())
}

fn read_transport_batch(
    reader: &mut impl Read,
    expected_source_stage: usize,
    token: Option<&str>,
    payload_buf: &mut Vec<f32>,
) -> io::Result<Option<TransportBatch>> {
    let mut source_stage_bytes = [0u8; 2];
    match reader.read_exact(&mut source_stage_bytes) {
        Ok(()) => {}
        Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(err) => return Err(err),
    }
    let source_stage = u16::from_le_bytes(source_stage_bytes) as usize;
    if source_stage != expected_source_stage {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "unexpected source stage in transport frame",
        ));
    }

    let mut batch_id_bytes = [0u8; 8];
    reader.read_exact(&mut batch_id_bytes)?;

    let mut tokens_bytes = [0u8; 4];
    reader.read_exact(&mut tokens_bytes)?;

    let mut payload_len_bytes = [0u8; 4];
    reader.read_exact(&mut payload_len_bytes)?;
    let payload_len = u32::from_le_bytes(payload_len_bytes) as usize;

    let mut tag_present_byte = [0u8; 1];
    reader.read_exact(&mut tag_present_byte)?;
    let tag_present = tag_present_byte[0] == 1;

    let mut received_tag = [0u8; 32];
    if tag_present {
        reader.read_exact(&mut received_tag)?;
    }
    payload_buf.resize(payload_len, 0.0);

    if cfg!(target_endian = "little") {
        // SAFETY: payload_buf points to initialized contiguous f32 memory; we reinterpret as bytes for I/O.
        let payload_bytes = unsafe {
            slice::from_raw_parts_mut(
                payload_buf.as_mut_ptr() as *mut u8,
                payload_len * std::mem::size_of::<f32>(),
            )
        };
        reader.read_exact(payload_bytes)?;
    } else {
        let mut payload_bytes = vec![0u8; payload_len * 4];
        reader.read_exact(&mut payload_bytes)?;
        for (i, chunk) in payload_bytes.chunks_exact(4).enumerate() {
            payload_buf[i] = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        }
    };

    let batch_id = u64::from_le_bytes(batch_id_bytes) as usize;
    let tokens_in_batch = u32::from_le_bytes(tokens_bytes) as usize;

    if let Some(t) = token {
        if !tag_present {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "transport authentication required but tag missing",
            ));
        }
        let expected_tag = auth_tag(source_stage, batch_id, tokens_in_batch, payload_buf, t);
        if received_tag != expected_tag {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "transport auth tag mismatch",
            ));
        }
    }

    Ok(Some(TransportBatch {
        batch_id,
        tokens_in_batch,
        payload: payload_buf.clone(),
    }))
}

/// Spawn a TCP transport bridge between pipeline stages.
///
/// In a multi-node deployment, the `bind_addr` and `connect_addr` can point to
/// different physical interfaces or remote hosts.
pub fn spawn_tcp_bridge(
    source_stage: usize,
    input_rx: mpsc::Receiver<TransportBatch>,
    output_tx: mpsc::SyncSender<TransportBatch>,
    config: TcpTransportConfig,
    listener: TcpListener,
    connect_addr: SocketAddr,
) -> thread::JoinHandle<BridgeAccumulator> {
    thread::spawn(move || {
        let client_stream = {
            let mut connected = None;
            let attempts = config.reconnect_attempts.max(1);
            for attempt in 0..attempts {
                match TcpStream::connect_timeout(&connect_addr, Duration::from_secs(1)) {
                    Ok(stream) => {
                        connected = Some(stream);
                        break;
                    }
                    Err(e) => {
                        tracing::debug!(
                            "TCP Bridge: Connection attempt {}/{} failed for {}: {}",
                            attempt + 1,
                            attempts,
                            connect_addr,
                            e
                        );
                        thread::sleep(Duration::from_millis(config.reconnect_backoff_ms));
                    }
                }
            }

            match connected {
                Some(stream) => stream,
                None => {
                    tracing::error!(
                        "TCP Bridge: Exhausted retries for {}. Falling back to passthrough.",
                        connect_addr
                    );
                    for batch in input_rx {
                        if output_tx.send(batch).is_err() {
                            break;
                        }
                    }
                    return BridgeAccumulator::default_with_stage(source_stage);
                }
            }
        };

        let (server_stream, _) = match listener.accept() {
            Ok(parts) => parts,
            Err(_) => {
                for batch in input_rx {
                    if output_tx.send(batch).is_err() {
                        break;
                    }
                }
                return BridgeAccumulator::default_with_stage(source_stage);
            }
        };

        let writer_auth_token = config.auth_token.clone();
        let reader_auth_token = config.auth_token.clone();

        let writer = thread::spawn(move || {
            let mut writer = BufWriter::with_capacity(64 * 1024, client_stream);
            let mut processed_batches = 0usize;
            let mut total_write_ms = 0.0_f32;
            let mut frame_buf = Vec::with_capacity(64 * 1024);
            for batch in input_rx {
                let write_start = Instant::now();
                if write_transport_batch(
                    &mut writer,
                    &batch,
                    source_stage,
                    writer_auth_token.as_deref(),
                    &mut frame_buf,
                )
                .is_err()
                {
                    break;
                }
                total_write_ms += write_start.elapsed().as_secs_f32() * 1000.0;
                processed_batches += 1;
            }
            let _ = writer.flush();
            let _ = writer.get_ref().shutdown(Shutdown::Write);
            (processed_batches, total_write_ms)
        });

        let mut reader = BufReader::with_capacity(64 * 1024, server_stream);
        let mut read_batches = 0usize;
        let mut total_read_ms = 0.0_f32;
        let mut payload_buf = Vec::with_capacity(16 * 1024);

        loop {
            let read_start = Instant::now();
            match read_transport_batch(
                &mut reader,
                source_stage,
                reader_auth_token.as_deref(),
                &mut payload_buf,
            ) {
                Ok(Some(batch)) => {
                    total_read_ms += read_start.elapsed().as_secs_f32() * 1000.0;
                    if output_tx.send(batch).is_err() {
                        break;
                    }
                    read_batches += 1;
                }
                Ok(None) => break,
                Err(_) => break,
            }
        }

        let (write_batches, total_write_ms) = writer.join().unwrap_or((0, 0.0));
        BridgeAccumulator {
            source_stage,
            processed_batches: read_batches.max(write_batches),
            total_write_ms,
            total_read_ms,
        }
    })
}

#[derive(Debug)]
pub struct BridgeAccumulator {
    pub source_stage: usize,
    pub processed_batches: usize,
    pub total_write_ms: f32,
    pub total_read_ms: f32,
}

impl BridgeAccumulator {
    fn default_with_stage(source_stage: usize) -> Self {
        Self {
            source_stage,
            processed_batches: 0,
            total_write_ms: 0.0,
            total_read_ms: 0.0,
        }
    }
}

/// Execute pipeline stages with real TCP loopback transport bridges between stages.
/// Execute pipeline stages distributed across the LAN using TCP transport.
/// Execute pipeline stages distributed across the LAN using TCP transport.
pub fn execute_pipeline_distributed(
    plan: &PipelinePlan,
    token_count: usize,
    micro_batch: usize,
    config: TcpTransportConfig,
    cluster: &crate::cluster::ClusterState,
    placement_context: Option<&crate::planning::PlacementPlan>,
    rebalance: Option<&crate::planning::RebalanceTrigger>,
) -> ExecutionResult {
    let stage_count = plan.stages.len();
    let micro_batch = micro_batch.max(1);
    let batch_count = token_count.div_ceil(micro_batch);
    if stage_count == 0 || token_count == 0 {
        return ExecutionResult {
            token_count,
            micro_batch,
            batch_count,
            stage_count,
            total_time_ms: 0.0,
            throughput_tokens_per_sec: 0.0,
            avg_token_latency_ms: 0.0,
            p95_token_latency_ms: 0.0,
            stage_stats: Vec::new(),
        };
    }

    #[derive(Debug)]
    struct StageAccumulator {
        stage_idx: usize,
        processed_batches: usize,
        total_compute_ms: f32,
        total_recv_wait_ms: f32,
        total_send_wait_ms: f32,
    }

    let (entry_tx, entry_rx) = mpsc::channel::<TransportBatch>();
    let mut stage_inputs: Vec<Option<mpsc::Receiver<TransportBatch>>> =
        Vec::with_capacity(stage_count);
    stage_inputs.push(Some(entry_rx));
    let mut stage_outputs = Vec::with_capacity(stage_count);
    let mut bridge_handles = Vec::new();
    let bridge_capacity = config.max_inflight_batches.max(1);

    for source_stage_idx in 0..stage_count.saturating_sub(1) {
        let (bridge_in_tx, bridge_in_rx) = mpsc::sync_channel::<TransportBatch>(bridge_capacity);
        let (bridge_out_tx, bridge_out_rx) = mpsc::sync_channel::<TransportBatch>(bridge_capacity);
        stage_outputs.push(bridge_in_tx);
        stage_inputs.push(Some(bridge_out_rx));

        let source_stage_p = &plan.stages[source_stage_idx];
        let target_stage_p = &plan.stages[source_stage_idx + 1];

        // Resolve network endpoints for the stages using ClusterState.
        // We bind a listener on the "target" node's logical interface and connect from the "source" node.
        let bind_ip = if let Some(m) = cluster.get_metrics(&target_stage_p.node_id) {
            m.ip_address
                .map(|sa| sa.ip())
                .unwrap_or_else(|| [127, 0, 0, 1].into())
        } else {
            [127, 0, 0, 1].into()
        };

        // Bind the listener immediately to reserve the port.
        let listener = TcpListener::bind(SocketAddr::new(bind_ip, 0))
            .map_err(|e| format!("Failed to bind listener on {}: {}", bind_ip, e))
            .expect("critical: failed to bind inter-stage bridge");
        let actual_addr = listener.local_addr().expect("failed to get local addr for listener");

        // For connection, if it's the same node, we can just use loopback.
        let connect_ip = if source_stage_p.node_id == target_stage_p.node_id {
            [127, 0, 0, 1].into()
        } else {
            actual_addr.ip()
        };
        let connect_addr = SocketAddr::new(connect_ip, actual_addr.port());

        bridge_handles.push(spawn_tcp_bridge(
            source_stage_idx,
            bridge_in_rx,
            bridge_out_tx,
            config.clone(),
            listener,
            connect_addr,
        ));
    }

    let (completion_tx, completion_rx) = mpsc::sync_channel::<TransportBatch>(bridge_capacity);
    stage_outputs.push(completion_tx);

    let mut stage_handles = Vec::with_capacity(stage_count);
    for stage_idx in 0..stage_count {
        let stage = plan.stages[stage_idx].clone();
        let rx = stage_inputs[stage_idx]
            .take()
            .expect("stage receiver should exist");
        let tx_next = stage_outputs[stage_idx].clone();

        let handle = thread::spawn(move || {
            let mut processed_batches = 0usize;
            let mut total_compute_ms = 0.0_f32;
            let mut total_recv_wait_ms = 0.0_f32;
            let mut total_send_wait_ms = 0.0_f32;

            loop {
                let recv_start = Instant::now();
                let Ok(mut batch) = rx.recv() else {
                    break;
                };
                total_recv_wait_ms += recv_start.elapsed().as_secs_f32() * 1000.0;

                let compute_start = Instant::now();
                run_stage_compute(&mut batch.payload, &stage);
                total_compute_ms += compute_start.elapsed().as_secs_f32() * 1000.0;

                let send_start = Instant::now();
                if tx_next.send(batch).is_err() {
                    break;
                }
                total_send_wait_ms += send_start.elapsed().as_secs_f32() * 1000.0;
                processed_batches += 1;
            }

            StageAccumulator {
                stage_idx,
                processed_batches,
                total_compute_ms,
                total_recv_wait_ms,
                total_send_wait_ms,
            }
        });

        stage_handles.push(handle);
    }

    drop(stage_outputs);

    let mut batch_started_at = vec![Instant::now(); batch_count];
    let exec_start = Instant::now();
    for (batch_idx, batch_started_slot) in batch_started_at.iter_mut().enumerate() {
        let batch_start_token = batch_idx * micro_batch;
        let tokens_in_batch = (token_count - batch_start_token).min(micro_batch);
        let payload_len = (tokens_in_batch.max(1) * 16).max(32);
        let payload = (0..payload_len)
            .map(|idx| (batch_idx as f32 * 0.01) + (idx as f32 * 0.0001))
            .collect();

        *batch_started_slot = Instant::now();
        let _ = entry_tx.send(TransportBatch {
            batch_id: batch_idx,
            tokens_in_batch,
            payload,
        });
    }
    drop(entry_tx);

    let mut token_latencies = Vec::with_capacity(token_count);
    for _ in 0..batch_count {
        let Ok(done_batch) = completion_rx.recv() else {
            break;
        };
        let started_at = batch_started_at[done_batch.batch_id];
        let batch_latency_ms = started_at.elapsed().as_secs_f32() * 1000.0;
        for _ in 0..done_batch.tokens_in_batch {
            token_latencies.push(batch_latency_ms);
        }
    }

    // Evaluate dynamic rebalance trigger during execution
    if let (Some(trigger), Some(placement)) = (rebalance, placement_context) {
        if let Some(migration) = trigger.evaluate(cluster, placement) {
            tracing::info!(
                "Distributed Runtime: Rebalance Triggered! Moving layers {:?} from {} to {}",
                migration.layers,
                migration.source_node,
                migration.target_node
            );
        }
    }
    let total_time_ms = exec_start.elapsed().as_secs_f32() * 1000.0;

    let mut stage_stats = Vec::with_capacity(stage_count);
    for handle in stage_handles {
        if let Ok(stats) = handle.join() {
            let divisor = stats.processed_batches.max(1) as f32;
            let avg_compute_ms = stats.total_compute_ms / divisor;

            // Feedback loop: update cluster state with actual compute latency.
            if let Some(stage) = plan.stages.get(stats.stage_idx) {
                cluster.get_metrics_mut(&stage.node_id, |m| {
                    m.record_latency(avg_compute_ms * 1000.0);
                });
            }

            stage_stats.push(StageExecutionStats {
                stage_idx: stats.stage_idx,
                processed_batches: stats.processed_batches,
                avg_compute_ms,
                avg_recv_wait_ms: stats.total_recv_wait_ms / divisor,
                avg_send_wait_ms: stats.total_send_wait_ms / divisor,
                avg_bridge_write_ms: 0.0,
                avg_bridge_read_ms: 0.0,
            });
        }
    }

    let mut bridge_stats = Vec::with_capacity(bridge_handles.len());
    for handle in bridge_handles {
        if let Ok(stats) = handle.join() {
            bridge_stats.push(stats);
        }
    }

    for bridge in bridge_stats {
        if let Some(stage) = stage_stats
            .iter_mut()
            .find(|s| s.stage_idx == bridge.source_stage)
        {
            let divisor = bridge.processed_batches.max(1) as f32;
            stage.avg_bridge_write_ms = bridge.total_write_ms / divisor;
            stage.avg_bridge_read_ms = bridge.total_read_ms / divisor;
        }
    }

    stage_stats.sort_by_key(|s| s.stage_idx);

    let throughput_tokens_per_sec = if total_time_ms > 0.0 {
        token_count as f32 / (total_time_ms / 1000.0)
    } else {
        0.0
    };

    let avg_token_latency_ms = if token_latencies.is_empty() {
        0.0
    } else {
        token_latencies.iter().sum::<f32>() / token_latencies.len() as f32
    };

    let mut sorted = token_latencies.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let p95_idx = ((sorted.len().saturating_sub(1)) as f32 * 0.95).round() as usize;
    let p95_token_latency_ms = sorted.get(p95_idx).copied().unwrap_or(0.0);

    ExecutionResult {
        token_count,
        micro_batch,
        batch_count,
        stage_count,
        total_time_ms,
        throughput_tokens_per_sec,
        avg_token_latency_ms,
        p95_token_latency_ms,
        stage_stats,
    }
}

pub fn execute_pipeline_tcp_loopback(
    plan: &PipelinePlan,
    token_count: usize,
    micro_batch: usize,
) -> ExecutionResult {
    execute_pipeline_tcp_loopback_with_config(
        plan,
        token_count,
        micro_batch,
        TcpTransportConfig::default(),
    )
}

/// Execute pipeline stages with real TCP loopback transport bridges and explicit transport config.
pub fn execute_pipeline_tcp_loopback_with_config(
    plan: &PipelinePlan,
    token_count: usize,
    micro_batch: usize,
    config: TcpTransportConfig,
) -> ExecutionResult {
    let stage_count = plan.stages.len();
    let micro_batch = micro_batch.max(1);
    let batch_count = token_count.div_ceil(micro_batch);
    if stage_count == 0 || token_count == 0 {
        return ExecutionResult {
            token_count,
            micro_batch,
            batch_count,
            stage_count,
            total_time_ms: 0.0,
            throughput_tokens_per_sec: 0.0,
            avg_token_latency_ms: 0.0,
            p95_token_latency_ms: 0.0,
            stage_stats: Vec::new(),
        };
    }

    #[derive(Debug)]
    struct StageAccumulator {
        stage_idx: usize,
        processed_batches: usize,
        total_compute_ms: f32,
        total_recv_wait_ms: f32,
        total_send_wait_ms: f32,
    }

    let (entry_tx, entry_rx) = mpsc::channel::<TransportBatch>();
    let mut stage_inputs: Vec<Option<mpsc::Receiver<TransportBatch>>> =
        Vec::with_capacity(stage_count);
    stage_inputs.push(Some(entry_rx));
    let mut stage_outputs = Vec::with_capacity(stage_count);
    let mut bridge_handles = Vec::new();
    let bridge_capacity = config.max_inflight_batches.max(1);

    for source_stage in 0..stage_count.saturating_sub(1) {
        let (bridge_in_tx, bridge_in_rx) = mpsc::sync_channel::<TransportBatch>(bridge_capacity);
        let (bridge_out_tx, bridge_out_rx) = mpsc::sync_channel::<TransportBatch>(bridge_capacity);
        stage_outputs.push(bridge_in_tx);
        stage_inputs.push(Some(bridge_out_rx));

        // For loopback execution, we use distinct ports for each inter-stage bridge.
        let loopback = [127, 0, 0, 1].into();
        let port = 0; // OS-assigned
        let bind_addr = SocketAddr::new(loopback, port);

        // Bind the listener immediately to reserve the port.
        let listener = TcpListener::bind(bind_addr).expect("failed to bind loopback listener");
        let actual_addr = listener.local_addr().expect("failed to get loopback local addr");

        bridge_handles.push(spawn_tcp_bridge(
            source_stage,
            bridge_in_rx,
            bridge_out_tx,
            config.clone(),
            listener,
            actual_addr,
        ));
    }

    let (completion_tx, completion_rx) = mpsc::sync_channel::<TransportBatch>(bridge_capacity);
    stage_outputs.push(completion_tx);

    let mut stage_handles = Vec::with_capacity(stage_count);
    for stage_idx in 0..stage_count {
        let stage = plan.stages[stage_idx].clone();
        let rx = stage_inputs[stage_idx]
            .take()
            .expect("stage receiver should exist");
        let tx_next = stage_outputs[stage_idx].clone();

        let handle = thread::spawn(move || {
            let mut processed_batches = 0usize;
            let mut total_compute_ms = 0.0_f32;
            let mut total_recv_wait_ms = 0.0_f32;
            let mut total_send_wait_ms = 0.0_f32;

            loop {
                let recv_start = Instant::now();
                let Ok(mut batch) = rx.recv() else {
                    break;
                };
                total_recv_wait_ms += recv_start.elapsed().as_secs_f32() * 1000.0;

                let compute_start = Instant::now();
                run_stage_compute(&mut batch.payload, &stage);
                total_compute_ms += compute_start.elapsed().as_secs_f32() * 1000.0;

                let send_start = Instant::now();
                if tx_next.send(batch).is_err() {
                    break;
                }
                total_send_wait_ms += send_start.elapsed().as_secs_f32() * 1000.0;
                processed_batches += 1;
            }

            StageAccumulator {
                stage_idx,
                processed_batches,
                total_compute_ms,
                total_recv_wait_ms,
                total_send_wait_ms,
            }
        });

        stage_handles.push(handle);
    }

    // Drop setup-time sender clones so channel closure can propagate and
    // stage/bridge threads can terminate once work is drained.
    drop(stage_outputs);

    let mut batch_started_at = vec![Instant::now(); batch_count];
    let exec_start = Instant::now();
    for (batch_idx, batch_started_slot) in batch_started_at.iter_mut().enumerate() {
        let batch_start_token = batch_idx * micro_batch;
        let tokens_in_batch = (token_count - batch_start_token).min(micro_batch);
        let payload_len = (tokens_in_batch.max(1) * 16).max(32);
        let payload = (0..payload_len)
            .map(|idx| (batch_idx as f32 * 0.01) + (idx as f32 * 0.0001))
            .collect();

        *batch_started_slot = Instant::now();
        if entry_tx
            .send(TransportBatch {
                batch_id: batch_idx,
                tokens_in_batch,
                payload,
            })
            .is_err()
        {
            break;
        }
    }
    drop(entry_tx);

    let mut token_latencies = Vec::with_capacity(token_count);
    for _ in 0..batch_count {
        let Ok(done_batch) = completion_rx.recv() else {
            break;
        };

        let started_at = batch_started_at
            .get(done_batch.batch_id)
            .copied()
            .unwrap_or(exec_start);
        let batch_latency_ms = started_at.elapsed().as_secs_f32() * 1000.0;
        for _ in 0..done_batch.tokens_in_batch {
            token_latencies.push(batch_latency_ms);
        }
    }
    let total_time_ms = exec_start.elapsed().as_secs_f32() * 1000.0;

    let mut stage_stats = Vec::with_capacity(stage_count);
    for handle in stage_handles {
        if let Ok(stats) = handle.join() {
            let divisor = stats.processed_batches.max(1) as f32;
            stage_stats.push(StageExecutionStats {
                stage_idx: stats.stage_idx,
                processed_batches: stats.processed_batches,
                avg_compute_ms: stats.total_compute_ms / divisor,
                avg_recv_wait_ms: stats.total_recv_wait_ms / divisor,
                avg_send_wait_ms: stats.total_send_wait_ms / divisor,
                avg_bridge_write_ms: 0.0,
                avg_bridge_read_ms: 0.0,
            });
        }
    }

    let mut bridge_stats = Vec::with_capacity(bridge_handles.len());
    for handle in bridge_handles {
        if let Ok(stats) = handle.join() {
            bridge_stats.push(stats);
        }
    }

    for bridge in bridge_stats {
        if let Some(stage) = stage_stats
            .iter_mut()
            .find(|s| s.stage_idx == bridge.source_stage)
        {
            let divisor = bridge.processed_batches.max(1) as f32;
            stage.avg_bridge_write_ms = bridge.total_write_ms / divisor;
            stage.avg_bridge_read_ms = bridge.total_read_ms / divisor;
        }
    }

    stage_stats.sort_by_key(|s| s.stage_idx);

    let throughput_tokens_per_sec = if total_time_ms > 0.0 {
        token_count as f32 / (total_time_ms / 1000.0)
    } else {
        0.0
    };

    let avg_token_latency_ms = if token_latencies.is_empty() {
        0.0
    } else {
        token_latencies.iter().sum::<f32>() / token_latencies.len() as f32
    };

    let mut sorted = token_latencies.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let p95_idx = ((sorted.len().saturating_sub(1)) as f32 * 0.95).round() as usize;
    let p95_token_latency_ms = sorted.get(p95_idx).copied().unwrap_or(0.0);

    ExecutionResult {
        token_count,
        micro_batch,
        batch_count,
        stage_count,
        total_time_ms,
        throughput_tokens_per_sec,
        avg_token_latency_ms,
        p95_token_latency_ms,
        stage_stats,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planning::LayerAssignment;
    use std::collections::HashMap;
    use std::io::Cursor;

    #[test]
    fn pipeline_plan_uses_node_device_mapping() {
        let assignments = vec![
            LayerAssignment::new("node-a".to_string(), 0, 10, 5.0),
            LayerAssignment::new("node-b".to_string(), 10, 20, 5.0),
        ];

        let mut devices = HashMap::new();
        devices.insert("node-a".to_string(), DeviceKind::Gpu);
        devices.insert("node-b".to_string(), DeviceKind::Cpu);

        let plan = PipelinePlan::from_assignments(&assignments, &devices);

        assert_eq!(plan.stages.len(), 2);
        assert_eq!(plan.stages[0].device, DeviceKind::Gpu);
        assert_eq!(plan.stages[1].device, DeviceKind::Cpu);
        assert!(plan.est_step_latency_ms() > 0.0);
    }

    #[test]
    fn schedule_size_matches_tokens_times_stages() {
        let schedule = build_token_schedule(3, 4);
        assert_eq!(schedule.len(), 12);
        assert_eq!(
            schedule[0],
            TokenStep {
                token_idx: 0,
                stage_idx: 0
            }
        );
        assert_eq!(
            schedule[11],
            TokenStep {
                token_idx: 3,
                stage_idx: 2
            }
        );
    }

    #[test]
    fn execution_reports_basic_stage_metrics() {
        let plan = PipelinePlan {
            stages: vec![
                StagePlacement {
                    node_id: "node-a".to_string(),
                    start_layer: 0,
                    end_layer: 10,
                    device: DeviceKind::Gpu,
                    est_latency_ms: 1.0,
                },
                StagePlacement {
                    node_id: "node-b".to_string(),
                    start_layer: 10,
                    end_layer: 20,
                    device: DeviceKind::Gpu,
                    est_latency_ms: 1.0,
                },
            ],
        };

        let result = execute_pipeline(&plan, 8, 1);
        assert_eq!(result.stage_count, 2);
        assert_eq!(result.token_count, 8);
        assert_eq!(result.micro_batch, 1);
        assert_eq!(result.batch_count, 8);
        assert_eq!(result.stage_stats.len(), 2);
        assert!(result.total_time_ms > 0.0);
        assert!(result.avg_token_latency_ms > 0.0);
        assert!(result.throughput_tokens_per_sec > 0.0);
    }

    #[test]
    fn execution_respects_micro_batch_count() {
        let plan = PipelinePlan {
            stages: vec![
                StagePlacement {
                    node_id: "node-a".to_string(),
                    start_layer: 0,
                    end_layer: 10,
                    device: DeviceKind::Gpu,
                    est_latency_ms: 1.0,
                },
                StagePlacement {
                    node_id: "node-b".to_string(),
                    start_layer: 10,
                    end_layer: 20,
                    device: DeviceKind::Gpu,
                    est_latency_ms: 1.0,
                },
            ],
        };

        let mb1 = execute_pipeline(&plan, 32, 1);
        let mb4 = execute_pipeline(&plan, 32, 4);

        assert_eq!(mb1.batch_count, 32);
        assert_eq!(mb4.batch_count, 8);
        assert!(mb4.total_time_ms > 0.0);
    }

    #[test]
    fn execution_supports_npu_stage_metrics() {
        let plan = PipelinePlan {
            stages: vec![
                StagePlacement {
                    node_id: "node-npu".to_string(),
                    start_layer: 0,
                    end_layer: 8,
                    device: DeviceKind::Npu,
                    est_latency_ms: 0.9,
                },
                StagePlacement {
                    node_id: "node-gpu".to_string(),
                    start_layer: 8,
                    end_layer: 16,
                    device: DeviceKind::Gpu,
                    est_latency_ms: 1.1,
                },
            ],
        };

        let result = execute_pipeline(&plan, 12, 3);
        assert_eq!(result.stage_count, 2);
        assert_eq!(result.batch_count, 4);
        assert_eq!(result.stage_stats.len(), 2);
        assert!(result.stage_stats.iter().any(|s| s.stage_idx == 0));
        assert!(result.total_time_ms > 0.0);
        assert!(result.throughput_tokens_per_sec > 0.0);
    }

    #[test]
    fn tcp_loopback_execution_reports_metrics() {
        let plan = PipelinePlan {
            stages: vec![
                StagePlacement {
                    node_id: "node-a".to_string(),
                    start_layer: 0,
                    end_layer: 10,
                    device: DeviceKind::Gpu,
                    est_latency_ms: 1.0,
                },
                StagePlacement {
                    node_id: "node-b".to_string(),
                    start_layer: 10,
                    end_layer: 20,
                    device: DeviceKind::Cpu,
                    est_latency_ms: 2.0,
                },
            ],
        };

        let result = execute_pipeline_tcp_loopback(&plan, 16, 2);
        assert_eq!(result.token_count, 16);
        assert_eq!(result.batch_count, 8);
        assert_eq!(result.stage_stats.len(), 2);
        assert!(result.total_time_ms > 0.0);
        assert!(result.throughput_tokens_per_sec > 0.0);
    }

    #[test]
    fn tcp_loopback_execution_with_hardening_config_reports_metrics() {
        let plan = PipelinePlan {
            stages: vec![
                StagePlacement {
                    node_id: "node-a".to_string(),
                    start_layer: 0,
                    end_layer: 12,
                    device: DeviceKind::Gpu,
                    est_latency_ms: 1.5,
                },
                StagePlacement {
                    node_id: "node-b".to_string(),
                    start_layer: 12,
                    end_layer: 24,
                    device: DeviceKind::Cpu,
                    est_latency_ms: 2.5,
                },
            ],
        };

        let result = execute_pipeline_tcp_loopback_with_config(
            &plan,
            24,
            3,
            TcpTransportConfig {
                max_inflight_batches: 4,
                reconnect_attempts: 4,
                reconnect_backoff_ms: 5,
                auth_token: Some("test-token".to_string()),
                ..Default::default()
            },
        );

        assert_eq!(result.token_count, 24);
        assert_eq!(result.batch_count, 8);
        assert_eq!(result.stage_stats.len(), 2);
        assert!(result.total_time_ms > 0.0);
        assert!(result.throughput_tokens_per_sec > 0.0);
    }

    #[test]
    fn transport_rejects_auth_mismatch_frames() {
        let batch = TransportBatch {
            batch_id: 7,
            tokens_in_batch: 4,
            payload: vec![0.1, 0.2, 0.3, 0.4],
        };
        let mut encoded = Vec::new();
        let mut frame_buf = Vec::new();
        write_transport_batch(&mut encoded, &batch, 0, Some("token-a"), &mut frame_buf)
            .expect("encode frame");

        let mut cursor = Cursor::new(encoded);
        let mut payload_buf = Vec::new();
        let err = read_transport_batch(&mut cursor, 0, Some("token-b"), &mut payload_buf)
            .expect_err("mismatched token should fail");
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    }

    #[test]
    fn transport_rejects_unexpected_source_stage() {
        let batch = TransportBatch {
            batch_id: 2,
            tokens_in_batch: 2,
            payload: vec![1.0, 2.0],
        };
        let mut encoded = Vec::new();
        let mut frame_buf = Vec::new();
        write_transport_batch(&mut encoded, &batch, 1, Some("token"), &mut frame_buf)
            .expect("encode frame");

        let mut cursor = Cursor::new(encoded);
        let mut payload_buf = Vec::new();
        let err = read_transport_batch(&mut cursor, 0, Some("token"), &mut payload_buf)
            .expect_err("source-stage mismatch should fail");
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    }
}
