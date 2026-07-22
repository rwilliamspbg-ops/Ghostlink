//! Host + inference metrics for the GUI System Performance tab.
//!
//! Host CPU/RAM are sampled on a background thread (sysinfo) so `/api/metrics`
//! never blocks on process spawns. GPU util is cached with short CLI timeouts.

use std::collections::VecDeque;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock, RwLock};
use std::thread;
use std::time::{Duration, Instant};

use sysinfo::{CpuRefreshKind, MemoryRefreshKind, RefreshKind, System};

const LATENCY_SAMPLES_CAP: usize = 256;
const HOST_SAMPLE_MS: u64 = 1500;
const GPU_CACHE_MS: u64 = 3000;

#[derive(Debug, Clone)]
pub struct HostSnapshot {
    pub cpu: f32,
    pub memory: f32,
    pub gpu: f32,
    pub gpu_available: bool,
    pub total_memory_gb: f32,
    pub used_memory_gb: f32,
    pub total_vram_gb: f32,
}

impl Default for HostSnapshot {
    fn default() -> Self {
        Self {
            cpu: 0.0,
            memory: 0.0,
            gpu: 0.0,
            gpu_available: false,
            total_memory_gb: 0.0,
            used_memory_gb: 0.0,
            total_vram_gb: 0.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct InferenceSnapshot {
    pub tokens_per_sec: f32,
    pub latency_p50_ms: f32,
    pub latency_p95_ms: f32,
    pub last_latency_ms: f32,
    pub last_tokens: u32,
    pub samples: usize,
    pub real_inference: bool,
}

impl Default for InferenceSnapshot {
    fn default() -> Self {
        Self {
            tokens_per_sec: 0.0,
            latency_p50_ms: 0.0,
            latency_p95_ms: 0.0,
            last_latency_ms: 0.0,
            last_tokens: 0,
            samples: 0,
            real_inference: false,
        }
    }
}

#[derive(Debug)]
pub struct InferenceMetrics {
    latency_ms: VecDeque<f32>,
    tokens_per_sec_ema: f32,
    last_latency_ms: f32,
    last_tokens: u32,
    last_real: bool,
}

impl Default for InferenceMetrics {
    fn default() -> Self {
        Self {
            latency_ms: VecDeque::with_capacity(LATENCY_SAMPLES_CAP),
            tokens_per_sec_ema: 0.0,
            last_latency_ms: 0.0,
            last_tokens: 0,
            last_real: false,
        }
    }
}

impl InferenceMetrics {
    pub fn record(
        &mut self,
        latency_ms: f32,
        tokens: u32,
        tokens_per_sec: Option<f32>,
        real: bool,
    ) {
        let latency_ms = latency_ms.max(0.1);
        self.last_latency_ms = latency_ms;
        self.last_tokens = tokens;
        self.last_real = real;

        if self.latency_ms.len() >= LATENCY_SAMPLES_CAP {
            self.latency_ms.pop_front();
        }
        self.latency_ms.push_back(latency_ms);

        let tps = tokens_per_sec.unwrap_or_else(|| {
            if latency_ms > 0.0 && tokens > 0 {
                (tokens as f32) / (latency_ms / 1000.0)
            } else {
                0.0
            }
        });
        if tps > 0.0 {
            if self.tokens_per_sec_ema <= 0.0 {
                self.tokens_per_sec_ema = tps;
            } else {
                // EMA — recent chats weigh more
                self.tokens_per_sec_ema = self.tokens_per_sec_ema * 0.65 + tps * 0.35;
            }
        }
    }

    pub fn snapshot(&self) -> InferenceSnapshot {
        let (p50, p95) = percentiles_ms(&self.latency_ms);
        InferenceSnapshot {
            tokens_per_sec: self.tokens_per_sec_ema,
            latency_p50_ms: if p50 > 0.0 { p50 } else { self.last_latency_ms },
            latency_p95_ms: if p95 > 0.0 {
                p95
            } else if self.last_latency_ms > 0.0 {
                self.last_latency_ms
            } else {
                0.0
            },
            last_latency_ms: self.last_latency_ms,
            last_tokens: self.last_tokens,
            samples: self.latency_ms.len(),
            real_inference: self.last_real,
        }
    }
}

fn percentiles_ms(samples: &VecDeque<f32>) -> (f32, f32) {
    if samples.is_empty() {
        return (0.0, 0.0);
    }
    let mut sorted: Vec<f32> = samples.iter().copied().collect();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let p50 = percentile_sorted(&sorted, 0.50);
    let p95 = percentile_sorted(&sorted, 0.95);
    (p50, p95)
}

fn percentile_sorted(sorted: &[f32], q: f32) -> f32 {
    if sorted.is_empty() {
        return 0.0;
    }
    if sorted.len() == 1 {
        return sorted[0];
    }
    let idx = ((sorted.len() as f32 - 1.0) * q.clamp(0.0, 1.0)).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

struct HostSamplerState {
    snap: RwLock<HostSnapshot>,
    started: AtomicBool,
    gpu_cache: Mutex<(Instant, f32, bool, f32)>, // ts, util, available, vram_gb
}

static HOST: OnceLock<HostSamplerState> = OnceLock::new();

fn host_state() -> &'static HostSamplerState {
    HOST.get_or_init(|| HostSamplerState {
        snap: RwLock::new(HostSnapshot::default()),
        started: AtomicBool::new(false),
        gpu_cache: Mutex::new((Instant::now() - Duration::from_secs(60), 0.0, false, 0.0)),
    })
}

/// Ensure background host sampling is running (idempotent).
pub fn ensure_host_sampler() {
    let state = host_state();
    if state
        .started
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }
    // Immediate sample so first /api/metrics is not empty.
    refresh_host_once();
    thread::Builder::new()
        .name("ghostlink-host-metrics".into())
        .spawn(|| loop {
            thread::sleep(Duration::from_millis(HOST_SAMPLE_MS));
            refresh_host_once();
        })
        .ok();
}

pub fn current_host_snapshot() -> HostSnapshot {
    ensure_host_sampler();
    host_state()
        .snap
        .read()
        .map(|g| g.clone())
        .unwrap_or_default()
}

fn refresh_host_once() {
    let mut sys = System::new_with_specifics(
        RefreshKind::nothing()
            .with_cpu(CpuRefreshKind::everything())
            .with_memory(MemoryRefreshKind::everything()),
    );
    // First refresh establishes baselines; second yields meaningful CPU %.
    sys.refresh_cpu_usage();
    thread::sleep(Duration::from_millis(200));
    sys.refresh_cpu_usage();
    sys.refresh_memory();

    let cpu = sys.global_cpu_usage().clamp(0.0, 100.0);
    let total = sys.total_memory() as f64;
    let used = sys.used_memory() as f64;
    let memory = if total > 0.0 {
        ((used / total) * 100.0).clamp(0.0, 100.0) as f32
    } else {
        0.0
    };
    let total_memory_gb = (total / (1024.0 * 1024.0 * 1024.0)) as f32;
    let used_memory_gb = (used / (1024.0 * 1024.0 * 1024.0)) as f32;

    let (gpu, gpu_available, total_vram_gb) = sample_gpu_cached();

    if let Ok(mut snap) = host_state().snap.write() {
        *snap = HostSnapshot {
            cpu,
            memory,
            gpu,
            gpu_available,
            total_memory_gb,
            used_memory_gb,
            total_vram_gb,
        };
    }
}

fn sample_gpu_cached() -> (f32, bool, f32) {
    let state = host_state();
    if let Ok(cache) = state.gpu_cache.lock() {
        if cache.0.elapsed() < Duration::from_millis(GPU_CACHE_MS) {
            return (cache.1, cache.2, cache.3);
        }
    }
    let (util, available, vram) = sample_gpu_util();
    if let Ok(mut cache) = state.gpu_cache.lock() {
        *cache = (Instant::now(), util, available, vram);
    }
    (util, available, vram)
}

fn sample_gpu_util() -> (f32, bool, f32) {
    if let Some((util, vram)) = try_nvidia_smi() {
        return (util, true, vram);
    }
    if let Some((util, vram)) = try_rocm_smi() {
        return (util, true, vram);
    }
    (0.0, false, 0.0)
}

fn try_nvidia_smi() -> Option<(f32, f32)> {
    let output = Command::new("nvidia-smi")
        .args([
            "--query-gpu=utilization.gpu,memory.total",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let line = text.lines().next()?.trim();
    // e.g. "12, 8192"
    let mut parts = line.split(',').map(|s| s.trim());
    let util: f32 = parts.next()?.parse().ok()?;
    let mem_mib: f32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0.0);
    Some((util.clamp(0.0, 100.0), mem_mib / 1024.0))
}

fn try_rocm_smi() -> Option<(f32, f32)> {
    // Best-effort: rocm-smi --showuse prints GPU use %
    let output = Command::new("rocm-smi")
        .args(["--showuse", "--showmeminfo", "vram"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut util = None;
    let mut vram_gb = 0.0_f32;
    for line in text.lines() {
        let lower = line.to_ascii_lowercase();
        if lower.contains("gpu use") || lower.contains("gpu%") {
            if let Some(num) = extract_first_number(line) {
                util = Some(num.clamp(0.0, 100.0));
            }
        }
        if lower.contains("total memory") || lower.contains("vram total") {
            if let Some(num) = extract_first_number(line) {
                // Often MiB
                vram_gb = if num > 256.0 { num / 1024.0 } else { num };
            }
        }
    }
    util.map(|u| (u, vram_gb))
}

fn extract_first_number(s: &str) -> Option<f32> {
    let mut buf = String::new();
    let mut seen_digit = false;
    for ch in s.chars() {
        if ch.is_ascii_digit() || (ch == '.' && seen_digit && !buf.contains('.')) {
            buf.push(ch);
            seen_digit = true;
        } else if seen_digit {
            break;
        }
    }
    if seen_digit {
        buf.parse().ok()
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentile_empty() {
        assert_eq!(percentiles_ms(&VecDeque::new()), (0.0, 0.0));
    }

    #[test]
    fn percentile_basic() {
        let mut q = VecDeque::new();
        for v in [10.0_f32, 20.0, 30.0, 40.0, 100.0] {
            q.push_back(v);
        }
        let (p50, p95) = percentiles_ms(&q);
        assert!((20.0..=40.0).contains(&p50));
        assert!(p95 >= 40.0);
    }

    #[test]
    fn inference_record_ema() {
        let mut m = InferenceMetrics::default();
        m.record(100.0, 50, Some(500.0), true);
        m.record(200.0, 40, Some(200.0), true);
        let s = m.snapshot();
        assert!(s.tokens_per_sec > 200.0 && s.tokens_per_sec < 500.0);
        assert!(s.latency_p50_ms > 0.0);
        assert!(s.samples == 2);
        assert!(s.real_inference);
    }
}
