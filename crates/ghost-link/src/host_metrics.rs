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
use std::time::{SystemTime, UNIX_EPOCH};

use sysinfo::{CpuRefreshKind, MemoryRefreshKind, RefreshKind, System};

const LATENCY_SAMPLES_CAP: usize = 256;
const METRICS_HISTORY_CAP: usize = 120;
const HOST_SAMPLE_MS: u64 = 1500;
const GPU_CACHE_MS: u64 = 3000;

#[derive(Debug, Clone, serde::Serialize)]
pub struct MetricsHistoryPoint {
    pub timestamp_ms: u64,
    pub throughput: f32,
    pub cpu: f32,
    pub memory: f32,
    pub gpu: f32,
    pub latency_p50: f32,
    pub latency_p95: f32,
    pub active_nodes: usize,
    pub inference_backend: String,
}

#[derive(Debug, Clone)]
pub struct HostSnapshot {
    pub cpu: f32,
    pub memory: f32,
    pub gpu: f32,
    pub gpu_available: bool,
    pub total_memory_gb: f32,
    pub used_memory_gb: f32,
    pub total_vram_gb: f32,
    /// GPU die temperature in °C, when the active probe reports one.
    /// `None` on the Windows generic-fallback path (`try_windows_gpu_counters`)
    /// — there's no standard cross-vendor perf counter for GPU temperature the
    /// way there is for utilization, only nvidia-smi/rocm-smi expose it.
    pub gpu_temp_c: Option<f32>,
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
            gpu_temp_c: None,
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
    /// Throughput Formula: throughput_tokens_per_sec = generated_tokens / (generation_seconds)
    /// where generation_seconds = latency_ms / 1000.0.
    /// Real inference runs update the 0.65/0.35 EMA. Failed, cancelled, or 0-token turns
    /// are recorded for latency tracking but excluded from throughput EMA so they do not drag it to zero.
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

        // Ignore 0-token or non-real attempts for throughput EMA & latency samples
        if tokens == 0 || !real {
            return;
        }

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
                // EMA — recent chats weigh more (65% prior EMA, 35% new sample)
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
    gpu_cache: Mutex<(Instant, f32, bool, f32, Option<f32>)>, // ts, util, available, vram_gb, temp_c
}

static HOST: OnceLock<HostSamplerState> = OnceLock::new();
static HISTORY: OnceLock<Mutex<VecDeque<MetricsHistoryPoint>>> = OnceLock::new();

fn host_state() -> &'static HostSamplerState {
    HOST.get_or_init(|| HostSamplerState {
        snap: RwLock::new(HostSnapshot::default()),
        started: AtomicBool::new(false),
        gpu_cache: Mutex::new((
            Instant::now() - Duration::from_secs(60),
            0.0,
            false,
            0.0,
            None,
        )),
    })
}

fn history_state() -> &'static Mutex<VecDeque<MetricsHistoryPoint>> {
    HISTORY.get_or_init(|| Mutex::new(VecDeque::with_capacity(METRICS_HISTORY_CAP)))
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

pub fn record_metrics_point(
    host: &HostSnapshot,
    inf: &InferenceSnapshot,
    active_nodes: usize,
    inference_backend: &str,
) {
    let timestamp_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    let point = MetricsHistoryPoint {
        timestamp_ms,
        throughput: inf.tokens_per_sec,
        cpu: host.cpu,
        memory: host.memory,
        gpu: host.gpu,
        latency_p50: inf.latency_p50_ms,
        latency_p95: inf.latency_p95_ms,
        active_nodes,
        inference_backend: inference_backend.to_string(),
    };

    if let Ok(mut history) = history_state().lock() {
        if history.len() >= METRICS_HISTORY_CAP {
            history.pop_front();
        }
        history.push_back(point);
    }
}

pub fn metrics_history(limit: usize) -> Vec<MetricsHistoryPoint> {
    let limit = limit.clamp(1, METRICS_HISTORY_CAP);
    history_state()
        .lock()
        .map(|history| {
            history
                .iter()
                .rev()
                .take(limit)
                .cloned()
                .collect::<Vec<_>>()
        })
        .map(|mut points| {
            points.reverse();
            points
        })
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

    let (gpu, gpu_available, total_vram_gb, gpu_temp_c) = sample_gpu_cached();

    if let Ok(mut snap) = host_state().snap.write() {
        *snap = HostSnapshot {
            cpu,
            memory,
            gpu,
            gpu_available,
            total_memory_gb,
            used_memory_gb,
            total_vram_gb,
            gpu_temp_c,
        };
    }
}

fn sample_gpu_cached() -> (f32, bool, f32, Option<f32>) {
    let state = host_state();
    if let Ok(cache) = state.gpu_cache.lock() {
        if cache.0.elapsed() < Duration::from_millis(GPU_CACHE_MS) {
            return (cache.1, cache.2, cache.3, cache.4);
        }
    }
    let (util, available, vram, temp_c) = sample_gpu_util();
    if let Ok(mut cache) = state.gpu_cache.lock() {
        *cache = (Instant::now(), util, available, vram, temp_c);
    }
    (util, available, vram, temp_c)
}

fn sample_gpu_util() -> (f32, bool, f32, Option<f32>) {
    if let Some((util, vram, temp_c)) = try_nvidia_smi() {
        return (util, true, vram, temp_c);
    }
    if let Some((util, vram, temp_c)) = try_rocm_smi() {
        return (util, true, vram, temp_c);
    }
    #[cfg(target_os = "windows")]
    if let Some((util, vram, temp_c)) = try_windows_gpu_counters() {
        return (util, true, vram, temp_c);
    }
    (0.0, false, 0.0, None)
}

/// Windows-native fallback GPU utilization for hosts with no nvidia-smi/rocm-smi
/// (i.e. DirectML/Vulkan GPUs — AMD and Intel iGPUs, most Windows laptops) using
/// the "GPU Engine" performance counter category, which Windows exposes for any
/// vendor with no CLI tool required.
///
/// Scoped to llama-server's own PID specifically, taking the max across *its*
/// engine instances (3D, Compute, Copy, Video — matches how Task Manager's
/// single GPU% figure picks the busiest engine for a process). An earlier
/// version took the max across ALL processes system-wide, which in practice
/// surfaced desktop-compositor (dwm.exe) or unrelated-app noise instead of the
/// inference workload — verified by comparing against a per-process query
/// while llama-server was actively generating (96.7% on its compute engine,
/// vs ~0.6% from unrelated processes the unscoped query happened to be
/// picking up). No llama-server process running just means 0% — not a probe
/// failure — so this still reports gpu_available=true (there IS a GPU here,
/// it's simply idle).
/// No temperature: Windows exposes GPU utilization for any vendor via the
/// "GPU Engine" perf counter category used below, but there's no equivalent
/// vendor-neutral counter for die temperature — that third tuple element is
/// always `None` here (nvidia-smi/rocm-smi, tried first in `sample_gpu_util`,
/// cover the vendors that do expose it).
#[cfg(target_os = "windows")]
fn try_windows_gpu_counters() -> Option<(f32, f32, Option<f32>)> {
    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "$p = Get-Process -Name llama-server -ErrorAction SilentlyContinue | Select-Object -First 1; \
             if ($p) { \
               $pat = \"*pid_$($p.Id)_*\"; \
               (Get-Counter '\\GPU Engine(*)\\Utilization Percentage' -ErrorAction Stop).CounterSamples \
                 | Where-Object { $_.Path -like $pat } \
                 | Measure-Object -Property CookedValue -Maximum \
                 | Select-Object -ExpandProperty Maximum \
             } else { 0 }",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let trimmed = text.trim();
    // Empty output means llama-server is running but currently has no active
    // engine instances (Measure-Object -Maximum with zero matches emits
    // nothing) — that's a legitimate 0%, not a parse failure.
    let util: f32 = if trimmed.is_empty() {
        0.0
    } else {
        trimmed.parse().ok()?
    };
    Some((util.clamp(0.0, 100.0), windows_vram_gb_cached(), None))
}

/// VRAM (really: usable GPU memory budget) for the Windows fallback path.
/// `Win32_VideoController.AdapterRAM` — the source `backend_registry.rs` uses
/// for the Settings-tab figure — is a known-unreliable static field on modern
/// Windows for integrated GPUs (frequently caps out around 4GB regardless of
/// actual shared-memory budget). llama-server's own Vulkan memory budget query
/// is accurate (verified: reports ~17GB free on a host where AdapterRAM claims
/// 4GB), so reuse it here rather than re-deriving GPU memory info a third way.
/// Cached for the process lifetime since total capacity doesn't change at runtime.
/// `pub(crate)` so `backend_registry.rs` can use the same accurate figure for
/// the Settings-tab backend card instead of the unreliable `AdapterRAM` value.
#[cfg(target_os = "windows")]
pub(crate) fn windows_vram_gb_cached() -> f32 {
    static VRAM_GB: OnceLock<f32> = OnceLock::new();
    *VRAM_GB.get_or_init(|| windows_vram_from_llama_list_devices().unwrap_or(0.0))
}

#[cfg(target_os = "windows")]
fn windows_vram_from_llama_list_devices() -> Option<f32> {
    let bin = std::env::var("GHOSTLINK_LLAMA_SERVER_BIN")
        .unwrap_or_else(|_| "third_party/llama.cpp/build/bin/Release/llama-server.exe".to_string());
    let output = Command::new(&bin).arg("--list-devices").output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    // e.g. "  Vulkan0: AMD Radeon(TM) 860M Graphics (18237 MiB, 17325 MiB free)"
    for line in text.lines() {
        if let Some(idx) = line.find("MiB free") {
            let before = &line[..idx];
            let segment = before.rsplit(',').next().unwrap_or(before);
            if let Some(mib) = extract_first_number(segment) {
                return Some(mib / 1024.0);
            }
        }
    }
    None
}

fn try_nvidia_smi() -> Option<(f32, f32, Option<f32>)> {
    let output = Command::new("nvidia-smi")
        .args([
            "--query-gpu=utilization.gpu,memory.total,temperature.gpu",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let line = text.lines().next()?.trim();
    // e.g. "12, 8192, 47"
    let mut parts = line.split(',').map(|s| s.trim());
    let util: f32 = parts.next()?.parse().ok()?;
    let mem_mib: f32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0.0);
    let temp_c: Option<f32> = parts.next().and_then(|s| s.parse().ok());
    Some((util.clamp(0.0, 100.0), mem_mib / 1024.0, temp_c))
}

fn try_rocm_smi() -> Option<(f32, f32, Option<f32>)> {
    // Best-effort: rocm-smi --showuse prints GPU use %, --showtemp prints
    // one or more "Temperature (Sensor ...) (C): NN.N" lines per GPU.
    let output = Command::new("rocm-smi")
        .args(["--showuse", "--showmeminfo", "vram", "--showtemp"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut util = None;
    let mut vram_gb = 0.0_f32;
    let mut temp_c = None;
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
        if lower.contains("temperature") {
            if let Some(num) = extract_first_number(line) {
                temp_c = Some(num);
            }
        }
    }
    util.map(|u| (u, vram_gb, temp_c))
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

    #[test]
    fn metrics_history_is_bounded_and_ordered() {
        let host = HostSnapshot::default();
        let inf = InferenceSnapshot {
            tokens_per_sec: 10.0,
            latency_p50_ms: 20.0,
            latency_p95_ms: 30.0,
            ..InferenceSnapshot::default()
        };

        for _ in 0..3 {
            record_metrics_point(&host, &inf, 1, "native");
        }

        let history = metrics_history(2);
        assert_eq!(history.len(), 2);
        assert!(history[0].timestamp_ms <= history[1].timestamp_ms);
        let history = metrics_history(2);
        assert_eq!(history.len(), 2);
        assert!(history[0].timestamp_ms <= history[1].timestamp_ms);
    }

    #[test]
    fn test_inference_metrics_ema_calculation() {
        let mut m = InferenceMetrics::default();
        // First record: 100 tokens in 2000 ms -> 50 tok/s
        m.record(2000.0, 100, None, true);
        assert!((m.tokens_per_sec_ema - 50.0).abs() < 0.01);
        assert_eq!(m.last_tokens, 100);

        // Second record: 100 tokens in 1000 ms -> 100 tok/s
        // EMA = 50.0 * 0.65 + 100.0 * 0.35 = 32.5 + 35.0 = 67.5
        m.record(1000.0, 100, None, true);
        assert!((m.tokens_per_sec_ema - 67.5).abs() < 0.01);
        assert_eq!(m.last_tokens, 100);
    }

    #[test]
    fn test_inference_metrics_cancelled_or_failed_sample_excluded() {
        let mut m = InferenceMetrics::default();
        m.record(1000.0, 50, None, true); // 50 tok/s
        let initial_ema = m.tokens_per_sec_ema;

        // Cancelled or non-real attempt (real: false, tokens: 0)
        m.record(500.0, 0, Some(0.0), false);
        assert_eq!(m.tokens_per_sec_ema, initial_ema);

        // Cancelled with tokens > 0 but real: false
        m.record(300.0, 15, Some(50.0), false);
        assert_eq!(m.tokens_per_sec_ema, initial_ema);
    }

    #[test]
    fn test_inference_metrics_zero_token_sample_excluded() {
        let mut m = InferenceMetrics::default();
        m.record(1000.0, 50, None, true); // 50 tok/s
        let initial_ema = m.tokens_per_sec_ema;
        assert!((initial_ema - 50.0).abs() < 0.01);

        // 0-token or failed attempt should NOT degrade EMA to zero
        m.record(500.0, 0, None, false);
        assert_eq!(m.tokens_per_sec_ema, initial_ema);
    }
}
