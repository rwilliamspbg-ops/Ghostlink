//! Native inference adapter for Ghost-Link.
//!
//! This is a launch-focused adapter that provides a stable native execution
//! interface while the full transformer runtime is being integrated.

#![allow(dead_code)]

use futures::Stream;
use std::fs;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::sleep as tokio_sleep;

/// Stream of incremental text deltas from a native backend's chat endpoint.
pub type NativeChatStream = Pin<Box<dyn Stream<Item = Result<String, String>> + Send>>;

#[derive(Debug, Clone)]
pub struct NativeGeneration {
    pub text: String,
    pub real_inference: bool,
    /// Tokens produced (when known from engine timings).
    pub tokens_generated: Option<u32>,
    /// Decode throughput tok/s when known.
    pub tokens_per_sec: Option<f32>,
    /// End-to-end generation latency in ms when known.
    pub latency_ms: Option<f32>,
}

impl NativeGeneration {
    fn text_only(text: String, real: bool) -> Self {
        Self {
            text,
            real_inference: real,
            tokens_generated: None,
            tokens_per_sec: None,
            latency_ms: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct NativeEngineClient {
    /// Shared, connection-pooled HTTP client. `reqwest::Client` is explicitly
    /// designed to be built once and reused — internally it's an `Arc` around
    /// the connection pool, so `.clone()` is a cheap refcount bump, not a new
    /// pool. Building a fresh client per request (the previous behavior)
    /// meant a brand-new TCP connection to llama-server on every single chat
    /// completion, with no keep-alive reuse at all.
    http: reqwest::Client,
}

// Static variable to track the llama-server process
static LLAMA_SERVER_PROCESS: OnceLock<Arc<Mutex<Option<Child>>>> = OnceLock::new();

// Cached local llama.cpp build fingerprint (short commit hash from
// `llama-server --version`). Never changes during a process's lifetime, so
// shelling out on every discovery/advertisement cycle would be wasteful.
// `None` means the binary is missing or `--version` failed/was unparsable —
// see `NativeEngineClient::get_llama_build_id`.
static LLAMA_BUILD_ID: OnceLock<Option<String>> = OnceLock::new();

impl NativeEngineClient {
    pub fn new() -> Self {
        Self {
            http: reqwest::Client::builder()
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
        }
    }

    /// Get or initialize the llama-server process handle
    fn get_process_handle() -> Arc<Mutex<Option<Child>>> {
        LLAMA_SERVER_PROCESS
            .get_or_init(|| Arc::new(Mutex::new(None)))
            .clone()
    }

    /// Walk upward from a starting directory looking for the Ghostlink project root.
    fn find_project_root() -> Option<PathBuf> {
        let mut roots = Vec::new();
        if let Ok(cwd) = std::env::current_dir() {
            roots.push(cwd);
        }
        if let Ok(exe) = std::env::current_exe() {
            if let Some(parent) = exe.parent() {
                roots.push(parent.to_path_buf());
            }
        }
        for mut dir in roots {
            for _ in 0..8 {
                let looks_like_root = dir.join("Cargo.toml").is_file()
                    && (dir.join("models").is_dir()
                        || dir.join("third_party").is_dir()
                        || dir.join("launch.sh").is_file());
                if looks_like_root {
                    return Some(dir);
                }
                if !dir.pop() {
                    break;
                }
            }
        }
        None
    }

    /// Resolve llama-server binary path with multi-location fallback.
    ///
    /// Priority:
    /// 1. `GHOSTLINK_LLAMA_SERVER_BIN` env var (must point to an existing file)
    /// 2. Common third_party / bin paths under project root and cwd
    /// 3. `<exe-dir>/llama-server` (side-by-side with the running binary)
    /// 4. `llama-server` on PATH
    fn get_llama_server_bin() -> String {
        // 1. Explicit env var
        if let Ok(bin) = std::env::var("GHOSTLINK_LLAMA_SERVER_BIN") {
            let bin = bin.trim().to_string();
            if !bin.is_empty() && Path::new(&bin).exists() {
                return bin;
            }
        }

        let relative_candidates = [
            "third_party/llama.cpp/build/bin/llama-server",
            "third_party/llama.cpp/build/bin/Release/llama-server.exe",
            "third_party/llama.cpp/build/bin/llama-server.exe",
            "bin/llama-server",
            "bin/llama-server.exe",
            "target/release/llama-server",
            "target/release/llama-server.exe",
            "target/debug/llama-server",
            "target/debug/llama-server.exe",
        ];

        let mut search_roots = Vec::new();
        if let Some(root) = Self::find_project_root() {
            search_roots.push(root);
        }
        if let Ok(cwd) = std::env::current_dir() {
            search_roots.push(cwd);
        }
        if let Ok(exe) = std::env::current_exe() {
            if let Some(parent) = exe.parent() {
                search_roots.push(parent.to_path_buf());
                // cargo target/{debug,release} -> repo root is two levels up
                if let Some(grand) = parent.parent().and_then(|p| p.parent()) {
                    search_roots.push(grand.to_path_buf());
                }
            }
        }

        for root in &search_roots {
            for rel in &relative_candidates {
                let candidate = root.join(rel);
                if candidate.is_file() {
                    return candidate.to_string_lossy().to_string();
                }
            }
            let side_by_side = root.join(if cfg!(windows) {
                "llama-server.exe"
            } else {
                "llama-server"
            });
            if side_by_side.is_file() {
                return side_by_side.to_string_lossy().to_string();
            }
        }

        // Final fallback to PATH
        if cfg!(windows) {
            "llama-server.exe".to_string()
        } else {
            "llama-server".to_string()
        }
    }

    /// Parse the short commit hash out of `llama-server --version`'s first
    /// line, e.g. `"version: 1 (da296d6)\nbuilt with MSVC ..."` -> `da296d6`.
    fn parse_llama_build_id(version_output: &str) -> Option<String> {
        let first_line = version_output.lines().next()?;
        let open = first_line.find('(')?;
        let close = first_line[open..].find(')')? + open;
        let hash = first_line[open + 1..close].trim();
        if hash.is_empty() {
            None
        } else {
            Some(hash.to_string())
        }
    }

    /// This node's local `llama.cpp` build fingerprint (the short commit hash
    /// `llama-server --version` prints in parens on its first line), cached
    /// for the process's lifetime since it never changes at runtime.
    ///
    /// Used to detect version-mismatched `ggml-rpc` peers before routing
    /// distributed inference through them — mismatched builds connect and
    /// exchange data over `ggml-rpc-server` without complaint, but can
    /// silently corrupt output for larger models while reporting healthy
    /// status the whole time (confirmed on real hardware; see
    /// `docs/BENCHMARKS.md`'s native two-machine entry and
    /// `rpc_cluster::discover_rpc_peers`).
    ///
    /// Best-effort: a missing binary or an unparsable `--version` output
    /// returns `None` rather than panicking or blocking startup, matching
    /// this codebase's established pattern for hardware/binary detection.
    pub fn get_llama_build_id() -> Option<String> {
        LLAMA_BUILD_ID
            .get_or_init(|| {
                let bin = Self::get_llama_server_bin();
                let output = match Command::new(&bin).arg("--version").output() {
                    Ok(output) => output,
                    Err(err) => {
                        eprintln!(
                            "[llama-build] Could not run '{bin} --version' to determine build \
                             fingerprint ({err}); version-mismatch detection for distributed \
                             inference will be skipped for this node."
                        );
                        return None;
                    }
                };
                let combined = format!(
                    "{}\n{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
                match Self::parse_llama_build_id(&combined) {
                    Some(build_id) => {
                        eprintln!("[llama-build] Detected local llama.cpp build: {build_id}");
                        Some(build_id)
                    }
                    None => {
                        eprintln!(
                            "[llama-build] '{bin} --version' output didn't contain a \
                             recognizable build hash; version-mismatch detection for \
                             distributed inference will be skipped for this node."
                        );
                        None
                    }
                }
            })
            .clone()
    }

    /// Raw URL from env/settings (may include `/completion` suffix from launchers).
    fn get_llama_server_url() -> String {
        std::env::var("GHOSTLINK_LLAMA_SERVER_URL")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .unwrap_or_else(|| "http://127.0.0.1:8080".to_string())
    }

    /// Normalize launcher URLs like `http://127.0.0.1:8080/completion` down to the
    /// server origin used for `/health`, `/v1/chat/completions`, etc.
    fn normalize_llama_base_url(url: &str) -> String {
        let mut base = url.trim().trim_end_matches('/').to_string();
        for suffix in [
            "/completion",
            "/v1/chat/completions",
            "/v1/completions",
            "/health",
        ] {
            if let Some(stripped) = base.strip_suffix(suffix) {
                base = stripped.trim_end_matches('/').to_string();
            }
        }
        if base.is_empty() {
            "http://127.0.0.1:8080".to_string()
        } else {
            base
        }
    }

    fn get_llama_base_url() -> String {
        Self::normalize_llama_base_url(&Self::get_llama_server_url())
    }

    /// Extra llama-server args: user env overrides, else perf-oriented defaults.
    /// Host/port/ngl/threads are set programmatically from the URL and settings.
    ///
    /// Defaults (local inference throughput):
    /// - `-fa` Flash Attention (lower bandwidth on long context)
    /// - `-b` / `-ub` batch sizes scaled by available VRAM
    fn get_llama_server_args() -> Vec<String> {
        if let Ok(v) = std::env::var("GHOSTLINK_LLAMA_SERVER_ARGS") {
            if !v.trim().is_empty() {
                eprintln!("[perf-tier] Using explicit GHOSTLINK_LLAMA_SERVER_ARGS override: {v}");
                return v.split_whitespace().map(|s| s.to_string()).collect();
            }
        }
        Self::default_perf_args()
    }

    /// Context size for llama-server (`-c`). Model default can be 128k+ which
    /// starves iGPU VRAM and tanks decode tok/s, so this scales with reported VRAM
    /// instead. Tiers were doubled from an earlier, tighter set of defaults after
    /// tool-calling chat (see `mcp::toolcall::format_observation`) showed 4096 was
    /// too tight for a single `fetch`-sized observation plus normal conversation —
    /// that observation path now truncates any single tool result, but headroom
    /// here still matters for multi-turn tool use within `MAX_TOOL_ITERATIONS`.
    ///
    /// `GHOSTLINK_CTX_SIZE` is an explicit, unconditional override — set it and
    /// you get exactly that value, full stop, same as always. Below that, the
    /// VRAM-tier default is further capped by `model_size_gb`: KV cache and
    /// model weights compete for the same finite memory on a unified-memory
    /// iGPU, so a VRAM tier sized for a small model can starve a large one.
    /// Found the hard way: a 13.6GB model + 16384 ctx (this function's old
    /// unconditional-16384-if-VRAM=8 behavior) left a 27.6GB host with under
    /// 1GB free — one more allocation away from OOM, not a hypothetical.
    fn get_ctx_size(model_size_gb: f32) -> u32 {
        if let Ok(val) = std::env::var("GHOSTLINK_CTX_SIZE") {
            if let Ok(n) = val.trim().parse::<u32>() {
                return n.clamp(512, 131072);
            }
        }
        let vram_tier = std::env::var("GHOSTLINK_VRAM_GB")
            .ok()
            .and_then(|v| v.trim().parse::<f32>().ok())
            .map(|vram| {
                if vram >= 16.0 {
                    32768
                } else if vram >= 12.0 {
                    16384
                } else if vram >= 8.0 {
                    8192
                } else {
                    4096
                }
            })
            .unwrap_or(8192);

        let model_cap = if model_size_gb >= 10.0 {
            4096
        } else if model_size_gb >= 5.0 {
            8192
        } else {
            u32::MAX
        };

        vram_tier.min(model_cap)
    }

    /// Batch size (`-b`) and micro-batch size (`-ub`) for prompt eval.
    ///
    /// Priority:
    /// 1. `GHOSTLINK_LLAMA_BATCH` / `GHOSTLINK_LLAMA_UBATCH` — explicit,
    ///    independently-settable overrides (either can be set without the
    ///    other; an unset or unparseable/zero value falls through to the
    ///    VRAM tier instead of producing a `-b 0` llama-server would reject)
    /// 2. VRAM-tier default (`docs/LOCAL_INFERENCE_TUNING.md`)
    fn get_batch_ubatch() -> (u32, u32) {
        let vram = std::env::var("GHOSTLINK_VRAM_GB")
            .ok()
            .and_then(|v| v.trim().parse::<f32>().ok())
            .unwrap_or(0.0);
        let (default_batch, default_ubatch) = if vram >= 12.0 {
            (2048, 512)
        } else if vram >= 8.0 {
            (1024, 512)
        } else if vram >= 4.0 {
            (512, 256)
        } else {
            (512, 128)
        };
        let batch = std::env::var("GHOSTLINK_LLAMA_BATCH")
            .ok()
            .and_then(|v| v.trim().parse::<u32>().ok())
            .filter(|&n| n > 0)
            .unwrap_or(default_batch);
        let ubatch = std::env::var("GHOSTLINK_LLAMA_UBATCH")
            .ok()
            .and_then(|v| v.trim().parse::<u32>().ok())
            .filter(|&n| n > 0)
            .unwrap_or(default_ubatch);
        (batch, ubatch)
    }

    /// KV cache quantization type for `-ctk`/`-ctv` — shared for K and V;
    /// this repo has never split them independently.
    ///
    /// Priority:
    /// 1. `GHOSTLINK_LLAMA_KV_CACHE_TYPE` — `"f16"`, `"q8_0"`, or `"q4_0"`;
    ///    any other value (typo, unsupported type) falls back to the
    ///    default rather than passing something llama-server might reject
    /// 2. `"q8_0"` — this repo's prior hardcoded default (~2x less cache
    ///    memory than f16, negligible quality cost for agent/tool loops)
    ///
    /// Only used when `get_flash_attention()` is true — llama.cpp requires
    /// Flash Attention for quantized KV cache, so `default_perf_args` skips
    /// `-ctk`/`-ctv` entirely when FA is off, regardless of this value.
    fn get_kv_cache_type() -> &'static str {
        match std::env::var("GHOSTLINK_LLAMA_KV_CACHE_TYPE")
            .ok()
            .as_deref()
            .map(str::trim)
        {
            Some("f16") => "f16",
            Some("q4_0") => "q4_0",
            _ => "q8_0",
        }
    }

    /// Whether to pass `-fa on` (Flash Attention).
    ///
    /// Priority:
    /// 1. `GHOSTLINK_LLAMA_FLASH_ATTN=0` (or `"off"`) — explicit opt-out
    /// 2. On — matches this repo's prior *unconditional* behavior exactly,
    ///    so nothing changes for anyone not setting this env var.
    fn get_flash_attention() -> bool {
        !matches!(
            std::env::var("GHOSTLINK_LLAMA_FLASH_ATTN").ok().as_deref(),
            Some("0") | Some("off")
        )
    }

    /// VRAM-aware batch defaults for prompt eval + Flash Attention + compact KV.
    fn default_perf_args() -> Vec<String> {
        let (batch, ubatch) = Self::get_batch_ubatch();
        let flash_attention = Self::get_flash_attention();
        let kv_cache_type = Self::get_kv_cache_type();

        let mut args = Vec::new();
        if flash_attention {
            args.push("-fa".to_string());
            args.push("on".to_string());
        }
        args.push("-b".to_string());
        args.push(batch.to_string());
        args.push("-ub".to_string());
        args.push(ubatch.to_string());
        if flash_attention {
            args.push("-ctk".to_string());
            args.push(kv_cache_type.to_string());
            args.push("-ctv".to_string());
            args.push(kv_cache_type.to_string());
        }

        // Cheap, greppable proof at boot that the VRAM-scaled tuning table in
        // docs/LOCAL_INFERENCE_TUNING.md actually landed for this hardware,
        // instead of only being inferable later from slow generations.
        eprintln!(
            "[perf-tier] -b {batch} -ub {ubatch} -fa {} -ctk/-ctv {}",
            if flash_attention { "on" } else { "off" },
            if flash_attention {
                kv_cache_type
            } else {
                "n/a (Flash Attention off)"
            }
        );
        args
    }

    /// Parse host and port from `GHOSTLINK_LLAMA_SERVER_URL`.
    /// Used both for spawning (--host, --port) and health checks so they can't drift.
    fn parse_host_port_from_url(url: &str) -> (String, u16) {
        let url = url.trim().trim_end_matches('/');
        let without_scheme = url
            .strip_prefix("https://")
            .or_else(|| url.strip_prefix("http://"))
            .unwrap_or(url);
        let host_port = without_scheme.split('/').next().unwrap_or(without_scheme);
        if let Some((host, port_str)) = host_port.rsplit_once(':') {
            let port = port_str.parse::<u16>().unwrap_or(8080);
            (host.to_string(), port)
        } else {
            (host_port.to_string(), 8080)
        }
    }

    /// Determine GPU offload layers (`-ngl`).
    ///
    /// Priority:
    /// 1. `GHOSTLINK_LLAMA_NGL` env var — explicit, unconditional, always wins
    /// 2. Auto-detect from `GHOSTLINK_VRAM_GB` env var (set by launch scripts),
    ///    further capped down for large models (see below)
    /// 3. `-1` — let llama-server decide (offload all layers it can)
    ///
    /// On a discrete GPU, offloading layers moves their weights out of system
    /// RAM into separate VRAM — a real memory *trade*. On a unified-memory
    /// iGPU (this function's main audience — see `launch-native.ps1`), "VRAM"
    /// is the same physical RAM, and measured directly on the reference
    /// hardware (AMD Radeon 860M, Vulkan backend): loading a 13.6GB model
    /// costs ~0.54GB of real (committed, non-reclaimable) memory CPU-only,
    /// ~7.35GB at a 24-layer partial offload, ~14.15GB at full offload —
    /// while decode throughput only went 6.48 -> 7.54 -> ~18 tok/s over that
    /// same range. The Vulkan device-local buffer is a genuine *second*
    /// allocation alongside the CPU-side one, not a move — offloading a large
    /// model on this hardware trades most of the system's free RAM for a
    /// modest speed gain, and at 24GB+ of duplicated weights left a 27.6GB
    /// host under 1GB free. So large models are capped toward CPU-only here,
    /// independent of the VRAM tier, unless `GHOSTLINK_LLAMA_NGL` overrides it.
    fn get_ngl(model_size_gb: f32) -> i32 {
        if let Ok(val) = std::env::var("GHOSTLINK_LLAMA_NGL") {
            if let Ok(n) = val.trim().parse::<i32>() {
                return n;
            }
        }
        let vram_tier = std::env::var("GHOSTLINK_VRAM_GB")
            .ok()
            .and_then(|v| v.trim().parse::<f32>().ok())
            .map(|vram| {
                if vram >= 12.0 {
                    40
                } else if vram >= 8.0 {
                    24
                } else if vram >= 4.0 {
                    12
                } else {
                    // Below 4GB VRAM, partial offload is likely to OOM on
                    // most 7B+ models — fall back to CPU-only rather than
                    // guessing a layer count that fits.
                    0
                }
            })
            .unwrap_or(-1);

        if model_size_gb >= 10.0 {
            0
        } else {
            vram_tier
        }
    }

    /// Whether to pass `--mlock` (pin model pages in RAM so they can't be
    /// swapped out).
    ///
    /// Priority:
    /// 1. `GHOSTLINK_MLOCK=1` / `=0` — explicit, unconditional override
    /// 2. RAM-tier default from `GHOSTLINK_SYSTEM_MEMORY_GB` (the same env
    ///    var `system_profile.rs` honors as its own RAM override): on only
    ///    when total system RAM is >=24GB
    /// 3. Off, if the RAM var isn't set — deliberately not falling back to
    ///    live OS detection here (e.g. `SystemProfile::detect_fast()`):
    ///    that call is 30s-cached process-wide, so a load-time decision made
    ///    through it could silently serve a stale answer, and mlock is risky
    ///    enough (see `launch.sh`'s reference-machine comment on this) that
    ///    guessing beats staying off only when there's no data at all.
    ///
    /// Mirrors `launch.sh`'s existing `GHOSTLINK_MLOCK`/`MLOCK_FLAG` heuristic
    /// ("mlock only when RAM is plentiful, avoids thrash on 16-32GB hosts
    /// under load") so GUI/API-driven loads (which go through this Rust path,
    /// not the shell launch scripts) get the same behavior instead of never
    /// setting `--mlock` at all.
    fn get_mlock() -> bool {
        match std::env::var("GHOSTLINK_MLOCK").ok().as_deref() {
            Some("1") => return true,
            Some("0") => return false,
            _ => {}
        }
        std::env::var("GHOSTLINK_SYSTEM_MEMORY_GB")
            .ok()
            .and_then(|v| v.trim().parse::<f32>().ok())
            .map(|ram_gb| ram_gb >= 24.0)
            .unwrap_or(false)
    }

    /// Whether to pass `--no-mmap` (read the whole model into memory upfront
    /// instead of mapping it and faulting pages in lazily).
    ///
    /// Opt-in only via `GHOSTLINK_NO_MMAP=1` — unlike `--mlock`, there's no
    /// measured hardware case in this repo showing a throughput win from
    /// disabling mmap by default, so it stays off unless explicitly
    /// requested rather than guessing at a heuristic.
    fn get_no_mmap() -> bool {
        matches!(
            std::env::var("GHOSTLINK_NO_MMAP").ok().as_deref(),
            Some("1")
        )
    }

    /// Determine thread count (`-t`).
    ///
    /// Priority:
    /// 1. `GHOSTLINK_LLAMA_THREADS` env var
    /// 2. `std::thread::available_parallelism()`
    /// 3. `4` (safe fallback)
    fn get_threads() -> usize {
        if let Ok(val) = std::env::var("GHOSTLINK_LLAMA_THREADS") {
            if let Ok(n) = val.trim().parse::<usize>() {
                return n.max(1);
            }
        }
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
            .max(1)
    }

    /// Number of parallel inference slots to give llama-server (`-np`).
    /// Defaults to `1` — today's exact prior behavior — unless
    /// `GHOSTLINK_PARALLEL_SLOTS` is set (mirrored from `RuntimeSettings
    /// .parallel_slots` by `load_settings()`, or set directly by a launch
    /// script). More than one slot lets llama-server serve concurrent
    /// generations instead of queueing them one at a time.
    fn get_parallel_slots() -> usize {
        if let Ok(val) = std::env::var("GHOSTLINK_PARALLEL_SLOTS") {
            if let Ok(n) = val.trim().parse::<usize>() {
                return n.clamp(1, 64);
            }
        }
        1
    }

    /// Check if llama-server is healthy. `url` may be a base URL or a launcher URL
    /// that includes `/completion`; both are normalized before probing `/health`.
    async fn check_llama_server_health(url: &str) -> bool {
        let base = Self::normalize_llama_base_url(url);
        let client = reqwest::Client::new();
        client
            .get(format!("{base}/health"))
            .timeout(Duration::from_secs(2))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    /// Model-ready timeout, in seconds, for a specific `llama-server` launch
    /// arg set. Real distributed (`--rpc`) loads take far longer than
    /// single-node loads of the same-sized file — confirmed this session:
    /// real cross-machine RPC loads took anywhere from 168s to over 900s
    /// depending on model size, while single-node loads of the same files
    /// completed comfortably within 90s (see `docs/BENCHMARKS.md`'s native
    /// two-machine entry). Raising the single-node timeout to match would
    /// make every single-node failure (e.g. a genuinely broken model file)
    /// take much longer to report, so the two cases get different defaults
    /// based on whether `--rpc` is actually present in `args` — the launch's
    /// own arg set, not a single global flag (this is checked per arg-set
    /// variant in `load_model_into_slot`'s staging loop, which tries
    /// multiple variants with/without quantized KV cache).
    ///
    /// `GHOSTLINK_MODEL_READY_TIMEOUT_SECS`, when set to a valid positive
    /// integer, overrides either default so an operator can tune this
    /// further without a rebuild.
    fn model_ready_timeout_secs(args: &[String]) -> u64 {
        if let Ok(val) = std::env::var("GHOSTLINK_MODEL_READY_TIMEOUT_SECS") {
            if let Ok(n) = val.trim().parse::<u64>() {
                if n > 0 {
                    return n;
                }
            }
        }
        let is_distributed = args.iter().any(|a| a == "--rpc");
        if is_distributed {
            600
        } else {
            90
        }
    }

    /// Wait for llama-server to become ready. Polls `child`'s exit status
    /// alongside the HTTP health check so a process that dies immediately
    /// (corrupt model file, missing shared lib, bad CLI arg) is reported in
    /// well under a second instead of only after the full timeout elapses.
    async fn wait_for_llama_server_ready(
        url: &str,
        timeout_secs: u64,
        child: &mut Child,
    ) -> Result<(), String> {
        let start = std::time::Instant::now();
        let timeout = Duration::from_secs(timeout_secs);
        let base = Self::normalize_llama_base_url(url);

        while start.elapsed() < timeout {
            if Self::check_llama_server_health(&base).await {
                return Ok(());
            }
            match child.try_wait() {
                Ok(Some(status)) => {
                    return Err(format!(
                        "llama-server exited before becoming ready (status: {status})"
                    ));
                }
                Ok(None) => {}
                Err(e) => {
                    return Err(format!("failed to poll llama-server process state: {e}"));
                }
            }
            tokio_sleep(Duration::from_millis(500)).await;
        }

        Err(format!(
            "llama-server did not become ready within {} seconds at {}/health",
            timeout_secs, base
        ))
    }

    /// Stop any llama-server we own, then free the listen port used by externally
    /// launched processes (launch.sh / launch-ollama.bat).
    fn stop_owned_llama_server() {
        let handle = Self::get_process_handle();
        let locked = handle.lock();
        if let Ok(mut guard) = locked {
            if let Some(mut child) = guard.take() {
                eprintln!(
                    "[model-load] Stopping owned llama-server process (PID: {:?})",
                    child.id()
                );
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }

    fn free_llama_port(port: u16) {
        eprintln!("[model-load] Freeing llama-server port {port}");
        if cfg!(windows) {
            let _ = Command::new("taskkill")
                .args(["/F", "/IM", "llama-server.exe"])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        } else {
            // Prefer precise port kill, then fall back to process name.
            let _ = Command::new("fuser")
                .args(["-k", &format!("{port}/tcp")])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
            let _ = Command::new("pkill")
                .args(["-f", "llama-server"])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
            // macOS / systems without fuser: best-effort via lsof
            if let Ok(output) = Command::new("lsof")
                .args(["-ti", &format!("tcp:{port}")])
                .output()
            {
                if output.status.success() {
                    let pids = String::from_utf8_lossy(&output.stdout);
                    for pid in pids.split_whitespace() {
                        let _ = Command::new("kill")
                            .args(["-9", pid])
                            .stdout(Stdio::null())
                            .stderr(Stdio::null())
                            .status();
                    }
                }
            }
        }
        std::thread::sleep(Duration::from_millis(400));
    }

    /// Resolve a model path that may be relative to the project root / cwd.
    fn resolve_model_path(model_path: &str) -> Result<PathBuf, String> {
        let direct = PathBuf::from(model_path);
        if direct.is_file() {
            return Ok(direct);
        }
        if let Ok(cwd) = std::env::current_dir() {
            let candidate = cwd.join(model_path);
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
        if let Some(root) = Self::find_project_root() {
            let candidate = root.join(model_path);
            if candidate.is_file() {
                return Ok(candidate);
            }
            // Also try models/<basename>
            if let Some(name) = Path::new(model_path).file_name() {
                let candidate = root.join("models").join(name);
                if candidate.is_file() {
                    return Ok(candidate);
                }
            }
        }
        Err(format!("model file not found: {model_path}"))
    }

    /// Find a free TCP port to stage a new llama-server on. Tries `preferred`
    /// first (for readable logs), then falls back to letting the OS assign one.
    /// There's an inherent bind-then-release race here (the port could be
    /// grabbed by something else before llama-server binds it), but that's
    /// the same best-effort tradeoff `free_llama_port` already makes.
    fn find_free_port(host: &str, preferred: u16) -> Option<u16> {
        use std::net::TcpListener;
        if let Ok(listener) = TcpListener::bind((host, preferred)) {
            if let Ok(addr) = listener.local_addr() {
                return Some(addr.port());
            }
        }
        if let Ok(listener) = TcpListener::bind((host, 0)) {
            if let Ok(addr) = listener.local_addr() {
                return Some(addr.port());
            }
        }
        None
    }

    /// Load a model into llama-server by restarting it with the new model.
    /// llama-server loads models at startup and doesn't support runtime hot-swapping,
    /// so we must restart it with the new model path.
    ///
    /// The new model is first staged on a scratch port and health-checked
    /// *before* the currently running server is touched. If the new model
    /// fails to load (bad file, OOM, slow CPU-only load exceeding the
    /// readiness timeout, etc.) the previous server keeps serving requests
    /// instead of the caller being left with no server running at all.
    ///
    /// `rpc_servers`/`tensor_split`, when both given non-empty, are passed
    /// straight through as llama-server's `--rpc`/`-ts` values — real
    /// cross-process model-parallel inference via llama.cpp's own RPC
    /// backend (see `crate::rpc_cluster`), not this crate's synthetic
    /// pipeline-benchmark transport. `None`/empty reproduces prior
    /// single-node behavior exactly.
    pub fn load_model_into_slot(
        &self,
        model_path: &str,
        rpc_servers: Option<&str>,
        tensor_split: Option<&str>,
    ) -> Result<(), String> {
        let resolved = Self::resolve_model_path(model_path)?;
        let normalized_path = resolved.to_string_lossy().replace('\\', "/");
        eprintln!("[model-load] Preparing to load model: {normalized_path}");

        let base_url = Self::get_llama_base_url();
        let (host, port) = Self::parse_host_port_from_url(&base_url);

        // Get binary and configuration
        let bin = Self::get_llama_server_bin();
        if bin != "llama-server" && bin != "llama-server.exe" && !Path::new(&bin).exists() {
            return Err(format!(
                "llama-server binary not found at '{bin}'. Set GHOSTLINK_LLAMA_SERVER_BIN."
            ));
        }
        let model_size_gb = fs::metadata(&resolved)
            .map(|m| m.len() as f32 / (1024.0 * 1024.0 * 1024.0))
            .unwrap_or(0.0);
        let ngl = Self::get_ngl(model_size_gb);
        let threads = Self::get_threads();
        let ctx = Self::get_ctx_size(model_size_gb);
        let parallel_slots = Self::get_parallel_slots();
        let mlock = Self::get_mlock();
        let no_mmap = Self::get_no_mmap();
        let mut extra_args = Self::get_llama_server_args();
        if let Some(servers) = rpc_servers.filter(|s| !s.is_empty()) {
            eprintln!("[model-load] Distributed inference enabled: --rpc {servers}");
            extra_args.push("--rpc".to_string());
            extra_args.push(servers.to_string());
            if let Some(split) = tensor_split.filter(|s| !s.is_empty()) {
                eprintln!("[model-load] Tensor split: -ts {split}");
                extra_args.push("-ts".to_string());
                extra_args.push(split.to_string());
            }
        }
        let alias = resolved
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("model")
            .to_string();

        // Build the command:
        //   llama-server -m <model> --alias <name> --host <host> --port <port> -c <ctx> [-ngl <n>] [-t <n>] -np <n> [--cont-batching]
        let build_cmd = |bind_port: u16, args: &[String]| -> Command {
            let mut cmd = Command::new(&bin);
            cmd.arg("-m").arg(&normalized_path);
            cmd.arg("--alias").arg(&alias);
            cmd.arg("--host").arg(&host);
            cmd.arg("--port").arg(bind_port.to_string());
            cmd.arg("-c").arg(ctx.to_string());
            cmd.arg("-np").arg(parallel_slots.to_string());
            if parallel_slots > 1 {
                // Only meaningful with more than one slot: lets llama-server
                // start decoding a newly-admitted request instead of
                // batching strictly by arrival order.
                cmd.arg("--cont-batching");
            }
            // Always pass -ngl, including -1 ("let llama-server decide /
            // offload all it can" per get_ngl()'s doc comment). Previously
            // this was gated on `ngl >= 0`, which silently omitted the flag
            // for -1 — and llama-server defaults -ngl to 0 (CPU-only) when
            // the flag isn't given at all, defeating the documented "auto"
            // intent on any launch path where GHOSTLINK_VRAM_GB/
            // GHOSTLINK_LLAMA_NGL aren't set. scripts/run_native_llama_server_stack.sh
            // and validate_native_llama_server.sh already pass `-ngl -1`
            // explicitly, confirming this build of llama-server accepts it.
            cmd.arg("-ngl").arg(ngl.to_string());
            cmd.arg("-t").arg(threads.to_string());
            if mlock {
                cmd.arg("--mlock");
            }
            if no_mmap {
                cmd.arg("--no-mmap");
            }
            for arg in args {
                cmd.arg(arg);
            }
            cmd.stdout(Stdio::null())
                .stderr(Stdio::null())
                .stdin(Stdio::null());
            cmd
        };

        // Fallback arg set with quantized KV cache (-ctk/-ctv) stripped: some
        // architectures fail to create a context with it (e.g. stories15M's
        // 48-dim attention heads don't divide evenly into q8_0's 32-element
        // blocks: "K cache type q8_0 ... does not divide n_embd_head_k=48").
        // Tried automatically below if the default args fail to load.
        let no_quant_kv_args: Vec<String> = {
            let mut out = Vec::new();
            let mut iter = extra_args.iter();
            while let Some(a) = iter.next() {
                if a == "-ctk" || a == "-ctv" {
                    iter.next();
                } else {
                    out.push(a.clone());
                }
            }
            out
        };
        let arg_variants: Vec<&Vec<String>> = if no_quant_kv_args.len() != extra_args.len() {
            vec![&extra_args, &no_quant_kv_args]
        } else {
            vec![&extra_args]
        };

        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| format!("failed to create async runtime: {e}"))?;

        // Stage the new model on a scratch port so the current server (if any)
        // keeps running while it loads.
        let staging_port = Self::find_free_port(&host, port.wrapping_add(1))
            .ok_or_else(|| "failed to find a free port to stage the new model".to_string())?;
        let staging_url = format!("http://{host}:{staging_port}");

        let mut winning_args: Option<&Vec<String>> = None;
        let mut last_err = String::new();
        for (attempt, args) in arg_variants.iter().enumerate() {
            eprintln!("[model-load] Staging model on port {staging_port}: {normalized_path}");
            let mut staging_cmd = build_cmd(staging_port, args);
            eprintln!(
                "[model-load] Command: {} {}",
                bin,
                staging_cmd
                    .get_args()
                    .map(|a| a.to_string_lossy())
                    .collect::<Vec<_>>()
                    .join(" ")
            );
            let mut staging_child = match staging_cmd.spawn() {
                Ok(c) => c,
                Err(err) => {
                    return Err(format!(
                        "failed to start llama-server ('{bin}'): {err}. Ensure the binary exists and port {staging_port} is free."
                    ));
                }
            };
            eprintln!(
                "[model-load] Staged llama-server PID: {:?}",
                staging_child.id()
            );

            let staging_timeout_secs = Self::model_ready_timeout_secs(args);
            let staged_ready = rt.block_on(Self::wait_for_llama_server_ready(
                &staging_url,
                staging_timeout_secs,
                &mut staging_child,
            ));
            let _ = staging_child.kill();
            let _ = staging_child.wait();
            match staged_ready {
                Ok(()) => {
                    winning_args = Some(args);
                    break;
                }
                Err(e) => {
                    last_err = e;
                    if attempt + 1 < arg_variants.len() {
                        eprintln!(
                            "[model-load] Staged load failed with this arg set ({last_err}); retrying without quantized KV cache."
                        );
                    }
                }
            }
        }

        let Some(winning_args) = winning_args else {
            eprintln!(
                "[model-load] New model failed to become ready ({last_err}); leaving previous server running."
            );
            return Err(last_err);
        };

        // New model verified healthy on the scratch port — safe to retire the
        // old server and rebind the real port. The OS page cache from the
        // staging load makes this second load fast.
        Self::stop_owned_llama_server();
        Self::free_llama_port(port);

        eprintln!("[model-load] Starting llama-server on {port}: {normalized_path}");
        let mut child = build_cmd(port, winning_args).spawn().map_err(|err| {
            format!(
                "failed to start llama-server ('{bin}'): {err}. Ensure the binary exists and port {port} is free."
            )
        })?;

        let pid = child.id();
        eprintln!("[model-load] Started llama-server with PID: {pid}");

        let final_timeout_secs = Self::model_ready_timeout_secs(winning_args);
        let ready = rt.block_on(Self::wait_for_llama_server_ready(
            &base_url,
            final_timeout_secs,
            &mut child,
        ));
        if let Err(e) = ready {
            let _ = child.kill();
            let _ = child.wait();
            Self::free_llama_port(port);
            return Err(e);
        }

        // Store the process handle now that it's confirmed healthy.
        let handle = Self::get_process_handle();
        if let Ok(mut guard) = handle.lock() {
            *guard = Some(child);
        }
        drop(handle);

        eprintln!("[model-load] Successfully loaded model: {normalized_path}");
        Ok(())
    }

    /// Unload the current model by stopping llama-server (owned or external).
    pub fn unload_model(&self) -> Result<(), String> {
        eprintln!("[model-unload] Unloading llama-server model");
        Self::stop_owned_llama_server();
        let base_url = Self::get_llama_base_url();
        let (_host, port) = Self::parse_host_port_from_url(&base_url);
        Self::free_llama_port(port);
        eprintln!("[model-unload] llama-server stopped");
        Ok(())
    }

    /// Check if a llama-server process is currently running
    pub fn has_running_llama_server(&self) -> bool {
        let handle = Self::get_process_handle();
        let locked = handle.lock();
        if let Ok(mut guard) = locked {
            match guard.as_mut() {
                // `try_wait` returns `Ok(None)` while the child is still
                // alive. `Ok(Some(_))` (or an error polling it) means the
                // process already exited (crash, OOM-kill, etc.) without
                // anyone reaping it yet — the stale handle must not be
                // reported as "running", or callers would believe a dead
                // server is still serving requests.
                Some(child) => matches!(child.try_wait(), Ok(None)),
                None => false,
            }
        } else {
            false
        }
    }

    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::too_many_arguments)]
    pub async fn generate(
        &self,
        model: &str,
        prompt: &str,
        max_tokens: usize,
        temperature: f32,
        top_p: f32,
        top_k: usize,
        repeat_penalty: f32,
        native_engine: &str,
        // Prior conversation turns (role, content), oldest first, *not*
        // including `prompt` itself (that's sent separately as the final
        // user turn). Empty for stateless call sites (the OpenAI-compatible
        // completions endpoints, tool-loop iterations after the first,
        // simulated/llama_cpp paths). Ignored by the llama_cpp/simulated
        // paths below, which build a single flat prompt string with no
        // chat-template concept.
        history: &[(String, String)],
        // See `generate_with_llama_server`'s doc comment. Ignored by the
        // llama_cpp/simulated paths below, which have no slot concept.
        id_slot: Option<i64>,
        cache_prompt: bool,
        // OpenAI-style `response_format` (e.g. `{"type": "json_schema", ...}`),
        // forwarded to llama-server's chat-completions endpoint when present.
        // Ignored by the llama_cpp/simulated paths, which have no grammar
        // support wired up.
        response_format: Option<serde_json::Value>,
    ) -> Result<NativeGeneration, String> {
        if model.trim().is_empty() {
            return Err("model cannot be empty".to_string());
        }

        let cleaned_prompt = prompt.trim();
        if cleaned_prompt.is_empty() {
            return Ok(NativeGeneration::text_only(
                format!(
                    "Native backend is ready for model '{}'. Provide a non-empty prompt for generation.",
                    model
                ),
                false,
            ));
        }

        let max_tokens = max_tokens.clamp(16, 4096);
        let started = std::time::Instant::now();

        match native_engine.trim().to_ascii_lowercase().as_str() {
            "llama_server" | "llama-server" => {
                let mut gen = self
                    .generate_with_llama_server(
                        model,
                        cleaned_prompt,
                        max_tokens,
                        temperature,
                        top_p,
                        top_k,
                        repeat_penalty,
                        history,
                        id_slot,
                        cache_prompt,
                        response_format,
                    )
                    .await?;
                if gen.latency_ms.is_none() {
                    gen.latency_ms = Some((started.elapsed().as_secs_f32() * 1000.0).max(0.1));
                }
                if gen.tokens_per_sec.is_none() {
                    if let (Some(lat), Some(toks)) = (gen.latency_ms, gen.tokens_generated) {
                        if lat > 0.0 && toks > 0 {
                            gen.tokens_per_sec = Some(toks as f32 / (lat / 1000.0));
                        }
                    }
                }
                Ok(gen)
            }
            "llama_cpp" | "llama.cpp" | "llama" => {
                let text = self.generate_with_llama_cpp(cleaned_prompt, max_tokens)?;
                let latency_ms = (started.elapsed().as_secs_f32() * 1000.0).max(0.1);
                let tokens_generated = (text.split_whitespace().count() as u32).max(1);
                Ok(NativeGeneration {
                    text,
                    real_inference: true,
                    tokens_generated: Some(tokens_generated),
                    tokens_per_sec: Some(tokens_generated as f32 / (latency_ms / 1000.0)),
                    latency_ms: Some(latency_ms),
                })
            }
            _ => self.generate_simulated(model, cleaned_prompt, max_tokens),
        }
    }

    fn generate_simulated(
        &self,
        model: &str,
        cleaned_prompt: &str,
        max_tokens: usize,
    ) -> Result<NativeGeneration, String> {
        let preview = cleaned_prompt
            .split_whitespace()
            .take(20)
            .collect::<Vec<_>>()
            .join(" ");

        Ok(NativeGeneration::text_only(
            format!(
                "[native:{}] generated response with token budget {}. Prompt preview: {}",
                model, max_tokens, preview
            ),
            false,
        ))
    }

    fn generate_with_llama_cpp(
        &self,
        cleaned_prompt: &str,
        max_tokens: usize,
    ) -> Result<String, String> {
        let bin = std::env::var("GHOSTLINK_LLAMA_CLI_BIN")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .unwrap_or_else(|| "llama-cli".to_string());
        let model_path = std::env::var("GHOSTLINK_MODEL_PATH")
            .map_err(|_| "GHOSTLINK_MODEL_PATH is required for llama_cpp mode".to_string())?;

        let output = Command::new(&bin)
            .arg("-m")
            .arg(model_path)
            .arg("-p")
            .arg(cleaned_prompt)
            .arg("-n")
            .arg(max_tokens.to_string())
            .arg("-no-cnv")
            .arg("-st")
            .arg("--no-display-prompt")
            .output()
            .map_err(|err| format!("failed to execute '{}': {}", bin, err))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("llama_cpp execution failed: {}", stderr.trim()));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let mut response = extract_generation_text(&stdout, &stderr, cleaned_prompt);
        if response.trim().is_empty() {
            let raw = if stdout.trim().is_empty() {
                stderr.trim()
            } else {
                stdout.trim()
            };
            if !raw.is_empty() {
                response = raw.to_string();
            }
        }
        if response.is_empty() {
            return Err("llama_cpp returned empty output".to_string());
        }

        Ok(response)
    }

    #[allow(clippy::too_many_arguments)]
    async fn generate_with_llama_server(
        &self,
        model: &str,
        cleaned_prompt: &str,
        max_tokens: usize,
        temperature: f32,
        top_p: f32,
        top_k: usize,
        repeat_penalty: f32,
        // Prior conversation turns (role, content), oldest first. Woven into
        // both the chat-completions `messages` array and the `/completion`
        // fallback's flat prompt, between the system prompt and the current
        // `cleaned_prompt` turn.
        history: &[(String, String)],
        // Slot/context reuse: `id_slot` pins this generation to a specific
        // llama-server slot (-1, llama-server's own "any idle slot" sentinel,
        // when None) and `cache_prompt` lets llama-server reuse whatever KV
        // state that slot already holds for the common prefix instead of
        // reprocessing it — the actual fix for repeat turns in the same
        // conversation otherwise re-evaluating the full prior transcript
        // every call. Both are llama-server's own request parameters
        // (confirmed current via its examples/server docs), not a Ghostlink
        // invention.
        id_slot: Option<i64>,
        cache_prompt: bool,
        response_format: Option<serde_json::Value>,
    ) -> Result<NativeGeneration, String> {
        let base_url = Self::get_llama_base_url();

        let timeout_secs = std::env::var("GHOSTLINK_LLAMA_SERVER_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(60)
            .clamp(5, 300);

        // Models have no clock; give them the current local date/time so
        // questions like "what date is it today?" get a correct answer.
        // Use portable chrono format (avoid %-d which is Unix-only).
        let system_prompt = format!(
            "You are a helpful assistant. Current local date and time: {}.",
            chrono::Local::now().format("%A, %B %d, %Y, %H:%M")
        );

        // Try chat completion endpoint first (for models with chat templates)
        let chat_url = format!("{base_url}/v1/chat/completions");

        let mut chat_messages =
            vec![serde_json::json!({"role": "system", "content": system_prompt})];
        for (role, content) in history {
            chat_messages.push(serde_json::json!({"role": role, "content": content}));
        }
        chat_messages.push(serde_json::json!({"role": "user", "content": cleaned_prompt}));

        let mut chat_payload = serde_json::json!({
            "model": model,
            "messages": chat_messages,
            "max_tokens": max_tokens,
            "temperature": temperature.clamp(0.0, 2.0),
            "top_p": top_p.clamp(0.0, 1.0),
            "top_k": top_k.clamp(1, 200),
            "repeat_penalty": repeat_penalty.clamp(0.0, 2.0),
            "stream": false,
            "id_slot": id_slot.unwrap_or(-1),
            "cache_prompt": cache_prompt
        });

        if let Some(response_format) = response_format {
            if let Some(obj) = chat_payload.as_object_mut() {
                obj.insert("response_format".to_string(), response_format);
                // Grammar-constrained decoding forces the final token stream
                // into schema shape regardless, but hybrid-reasoning models
                // (e.g. Qwen3.5) otherwise spend the entire max_tokens budget
                // on <think> content before ever reaching it — confirmed live
                // against llama-server: with thinking on, a 400-token budget
                // ran out mid-reasoning and returned empty content; with it
                // off, the very next call returned valid schema JSON in 22
                // tokens. Non-reasoning models simply ignore this field.
                obj.insert(
                    "chat_template_kwargs".to_string(),
                    serde_json::json!({ "enable_thinking": false }),
                );
            }
        }

        // Reuse the shared, connection-pooled client instead of building a new
        // one (and a new TCP connection) per request; apply the configurable
        // timeout per-request instead of baking it into the client so this
        // still behaves exactly as before if GHOSTLINK_LLAMA_SERVER_TIMEOUT_SECS
        // changes between calls.
        let client = self.http.clone();
        let request_timeout = Duration::from_secs(timeout_secs);

        // Try chat endpoint first
        let chat_response = client
            .post(&chat_url)
            .header("Content-Type", "application/json")
            .json(&chat_payload)
            .timeout(request_timeout)
            .send()
            .await;

        if let Ok(response) = chat_response {
            if response.status().is_success() {
                let parsed: serde_json::Value = response
                    .json()
                    .await
                    .map_err(|e| format!("invalid llama_server JSON response: {}", e))?;

                if let Some(gen) = generation_from_llama_json(&parsed) {
                    return Ok(gen);
                }
            } else if response.status().as_u16() == 400 {
                // Fall through to completion endpoint if chat fails with 400
            }
        }

        // Fall back to completion endpoint for models without chat template
        let completion_url = format!("{base_url}/completion");

        // Format prompt for completion endpoint: system + prior turns + user.
        // No real chat template here (this is the no-template fallback), so
        // prior turns are just labelled plain-text lines - good enough for
        // models that fell through to this path specifically because they
        // don't understand structured `messages`.
        let mut completion_prompt = system_prompt.clone();
        for (role, content) in history {
            let label = if role.eq_ignore_ascii_case("assistant") {
                "Assistant"
            } else {
                "User"
            };
            completion_prompt.push_str(&format!("\n\n{label}: {content}"));
        }
        completion_prompt.push_str(&format!("\n\nUser: {cleaned_prompt}\n\nAssistant:"));

        let completion_payload = serde_json::json!({
            "model": model,
            "prompt": completion_prompt,
            "max_tokens": max_tokens,
            "temperature": temperature.clamp(0.0, 2.0),
            "top_p": top_p.clamp(0.0, 1.0),
            "top_k": top_k.clamp(1, 200),
            "repeat_penalty": repeat_penalty.clamp(0.0, 2.0),
            "stream": false,
            "id_slot": id_slot.unwrap_or(-1),
            "cache_prompt": cache_prompt
        });

        let response = client
            .post(&completion_url)
            .header("Content-Type", "application/json")
            .json(&completion_payload)
            .timeout(request_timeout)
            .send()
            .await
            .map_err(|e| format!("llama_server request failed: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(format!(
                "llama_server request failed with status {}: {}",
                status, error_text
            ));
        }

        let parsed: serde_json::Value = response
            .json()
            .await
            .map_err(|e| format!("invalid llama_server JSON response: {}", e))?;

        if let Some(gen) = generation_from_llama_json(&parsed) {
            return Ok(gen);
        }

        Err("llama_server returned empty content".to_string())
    }

    /// Real incremental streaming against llama-server's OpenAI-compatible
    /// chat endpoint. Unlike `generate_with_llama_server` (which always sends
    /// `"stream": false` and returns only the complete text), this forwards
    /// text deltas to the caller as llama-server produces them.
    ///
    /// Only covers the chat-completions endpoint (the common case for models
    /// with a chat template, which is what this project's models use in
    /// practice). If the model has no chat template (chat endpoint returns
    /// HTTP 400), falls back to the existing non-streaming `/completion`
    /// path and yields the whole result as a single chunk rather than
    /// duplicating a second incremental parser for an uncommon case.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::too_many_arguments)]
    pub async fn generate_chat_stream(
        &self,
        model: &str,
        cleaned_prompt: &str,
        max_tokens: usize,
        temperature: f32,
        top_p: f32,
        top_k: usize,
        repeat_penalty: f32,
        id_slot: Option<i64>,
        cache_prompt: bool,
    ) -> Result<NativeChatStream, String> {
        use futures::StreamExt;

        let base_url = Self::get_llama_base_url();
        // Time to wait for llama-server to accept the request and start
        // responding (covers prompt prefill on a cold/uncached slot), not
        // the total generation time.
        let connect_timeout_secs = std::env::var("GHOSTLINK_LLAMA_CONNECT_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(30)
            .clamp(5, 120);
        // Max gap allowed between successive SSE chunks once streaming has
        // started. This is deliberately NOT a cap on total generation time —
        // a long answer that keeps producing tokens should never be killed
        // just for running past a fixed wall-clock budget; only a stalled
        // connection (no bytes at all for this long) should be.
        let idle_timeout_secs = std::env::var("GHOSTLINK_LLAMA_SERVER_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(60)
            .clamp(5, 300);
        let system_prompt = format!(
            "You are a helpful assistant. Current local date and time: {}.",
            chrono::Local::now().format("%A, %B %d, %Y, %H:%M")
        );
        let chat_url = format!("{base_url}/v1/chat/completions");
        let chat_payload = serde_json::json!({
            "model": model,
            "messages": [
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": cleaned_prompt}
            ],
            "max_tokens": max_tokens,
            "temperature": temperature.clamp(0.0, 2.0),
            "top_p": top_p.clamp(0.0, 1.0),
            "top_k": top_k.clamp(1, 200),
            "repeat_penalty": repeat_penalty.clamp(0.0, 2.0),
            "stream": true,
            "id_slot": id_slot.unwrap_or(-1),
            "cache_prompt": cache_prompt
        });

        let client = self.http.clone();
        let connect_timeout = Duration::from_secs(connect_timeout_secs);
        let idle_timeout = Duration::from_secs(idle_timeout_secs);

        let response = tokio::time::timeout(
            connect_timeout,
            client
                .post(&chat_url)
                .header("Content-Type", "application/json")
                .json(&chat_payload)
                .send(),
        )
        .await
        .map_err(|_| {
            format!(
                "llama_server streaming request timed out after {connect_timeout_secs}s waiting for a response"
            )
        })?
        .map_err(|e| format!("llama_server streaming request failed: {}", e))?;

        if response.status().as_u16() == 400 {
            // No chat template on this model: fall back to the existing
            // non-streaming completion path and present it as one chunk.
            let gen = self
                .generate_with_llama_server(
                    model,
                    cleaned_prompt,
                    max_tokens,
                    temperature,
                    top_p,
                    top_k,
                    repeat_penalty,
                    &[],
                    id_slot,
                    cache_prompt,
                    None,
                )
                .await?;
            let single = futures::stream::once(async move { Ok(gen.text) });
            return Ok(Box::pin(single));
        }

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!(
                "llama_server streaming request failed with status {}: {}",
                status, body
            ));
        }

        let (tx, rx) = mpsc::channel::<Result<String, String>>(100);
        tokio::spawn(async move {
            let mut byte_stream = response.bytes_stream();
            let mut buf = String::new();
            loop {
                let chunk = match tokio::time::timeout(idle_timeout, byte_stream.next()).await {
                    Ok(Some(Ok(c))) => c,
                    Ok(Some(Err(e))) => {
                        let _ = tx.send(Err(format!("stream read error: {e}"))).await;
                        return;
                    }
                    Ok(None) => return, // stream ended normally
                    Err(_) => {
                        let _ = tx
                            .send(Err(format!(
                                "stream idle timeout: no data from llama_server for {idle_timeout_secs}s"
                            )))
                            .await;
                        return;
                    }
                };
                buf.push_str(&String::from_utf8_lossy(&chunk));
                while let Some(pos) = buf.find('\n') {
                    let line = buf[..pos].trim().to_string();
                    buf.drain(..=pos);
                    if line.is_empty() {
                        continue;
                    }
                    let payload = match line.strip_prefix("data: ") {
                        Some(p) => p,
                        None => continue,
                    };
                    if payload == "[DONE]" {
                        return;
                    }
                    let data: serde_json::Value = match serde_json::from_str(payload) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };
                    let delta = data
                        .get("choices")
                        .and_then(|c| c.get(0))
                        .and_then(|c| c.get("delta"))
                        .and_then(|d| d.get("content"))
                        .and_then(|v| v.as_str());
                    if let Some(text) = delta {
                        if !text.is_empty() && tx.send(Ok(text.to_string())).await.is_err() {
                            return; // receiver dropped, stop reading
                        }
                    }
                    let finished = data
                        .get("choices")
                        .and_then(|c| c.get(0))
                        .and_then(|c| c.get("finish_reason"))
                        .map(|v| !v.is_null())
                        .unwrap_or(false);
                    if finished {
                        return;
                    }
                }
            }
        });

        Ok(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }
}

fn generation_from_llama_json(parsed: &serde_json::Value) -> Option<NativeGeneration> {
    let mut text = parsed
        .get("content")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    if text.is_none() {
        text = parsed
            .get("choices")
            .and_then(|choices| choices.get(0))
            .and_then(|c| {
                c.get("text")
                    .or_else(|| c.get("message").and_then(|m| m.get("content")))
            })
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
    }

    let text = text?;
    let (tokens_generated, tokens_per_sec, latency_ms) = parse_llama_timings(parsed, &text);
    Some(NativeGeneration {
        text,
        real_inference: true,
        tokens_generated,
        tokens_per_sec,
        latency_ms,
    })
}

fn parse_llama_timings(
    parsed: &serde_json::Value,
    text: &str,
) -> (Option<u32>, Option<f32>, Option<f32>) {
    let timings = parsed.get("timings");
    let predicted_n = timings
        .and_then(|t| t.get("predicted_n"))
        .and_then(|v| v.as_u64())
        .or_else(|| {
            parsed
                .get("tokens_predicted")
                .and_then(|v| v.as_u64())
                .or_else(|| parsed.get("tokens_evaluated").and_then(|v| v.as_u64()))
        })
        .map(|n| n as u32)
        .or_else(|| {
            let words = text.split_whitespace().count() as u32;
            if words > 0 {
                Some(words)
            } else {
                None
            }
        });

    let predicted_ms = timings
        .and_then(|t| t.get("predicted_ms"))
        .and_then(|v| v.as_f64())
        .map(|ms| ms as f32)
        .or_else(|| {
            timings
                .and_then(|t| t.get("predicted_per_second"))
                .and_then(|v| v.as_f64())
                .and_then(|tps| {
                    predicted_n.map(|n| {
                        if tps > 0.0 {
                            (n as f32) / (tps as f32) * 1000.0
                        } else {
                            0.0
                        }
                    })
                })
        });

    let tokens_per_sec = timings
        .and_then(|t| t.get("predicted_per_second"))
        .and_then(|v| v.as_f64())
        .map(|v| v as f32)
        .or_else(|| match (predicted_n, predicted_ms) {
            (Some(n), Some(ms)) if ms > 0.0 && n > 0 => Some(n as f32 / (ms / 1000.0)),
            _ => None,
        });

    (predicted_n, tokens_per_sec, predicted_ms)
}

fn extract_generation_text(stdout: &str, stderr: &str, prompt: &str) -> String {
    let candidate = if stdout.trim().is_empty() {
        stderr
    } else {
        stdout
    };

    let mut kept = Vec::new();
    for raw in candidate.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }

        if line.starts_with("Loading model")
            || line.starts_with("build")
            || line.starts_with("model")
            || line.starts_with("ftype")
            || line.starts_with("modalities")
            || line.starts_with("available commands")
            || line.starts_with("/exit")
            || line.starts_with("/regen")
            || line.starts_with("/clear")
            || line.starts_with("/read")
            || line.starts_with("/glob")
            || line.starts_with("[ Prompt:")
            || line.starts_with("Exiting")
            || line.contains('█')
        {
            continue;
        }

        if let Some(rest) = line.strip_prefix('>') {
            let prompt_line = rest.trim();
            if prompt_line.eq_ignore_ascii_case(prompt) {
                continue;
            }
            if prompt_line.is_empty() {
                continue;
            }
            kept.push(prompt_line.to_string());
            continue;
        }

        kept.push(line.to_string());
    }

    kept.join(" ")
}

#[cfg(test)]
mod tests {
    use super::NativeEngineClient;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        ENV_LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn parse_llama_build_id_extracts_short_hash_from_first_line() {
        let output = "version: 1 (da296d6)\nbuilt with MSVC 19.50.35725.0 for x64";
        assert_eq!(
            NativeEngineClient::parse_llama_build_id(output),
            Some("da296d6".to_string())
        );
    }

    #[test]
    fn parse_llama_build_id_handles_single_line_output() {
        assert_eq!(
            NativeEngineClient::parse_llama_build_id("version: 1 (e920c523e)"),
            Some("e920c523e".to_string())
        );
    }

    #[test]
    fn parse_llama_build_id_returns_none_without_parens() {
        assert_eq!(
            NativeEngineClient::parse_llama_build_id("no version info here"),
            None
        );
    }

    #[test]
    fn parse_llama_build_id_returns_none_on_empty_parens() {
        assert_eq!(
            NativeEngineClient::parse_llama_build_id("version: 1 ()"),
            None
        );
    }

    #[test]
    fn parse_llama_build_id_returns_none_on_empty_input() {
        assert_eq!(NativeEngineClient::parse_llama_build_id(""), None);
    }

    #[test]
    fn get_llama_build_id_does_not_panic_when_binary_is_missing() {
        let _guard = env_lock().lock().expect("env lock poisoned");
        // Best-effort: should gracefully return None (or a cached Some from
        // an earlier test run in this process) rather than panic, even when
        // no llama-server binary is resolvable in this test environment.
        let _ = NativeEngineClient::get_llama_build_id();
    }

    #[test]
    fn model_ready_timeout_uses_90s_for_single_node_args() {
        let _guard = env_lock().lock().expect("env lock poisoned");
        std::env::remove_var("GHOSTLINK_MODEL_READY_TIMEOUT_SECS");
        let args = vec!["-fa".to_string()];
        assert_eq!(NativeEngineClient::model_ready_timeout_secs(&args), 90);
    }

    #[test]
    fn model_ready_timeout_uses_600s_when_rpc_flag_present() {
        let _guard = env_lock().lock().expect("env lock poisoned");
        std::env::remove_var("GHOSTLINK_MODEL_READY_TIMEOUT_SECS");
        let args = vec![
            "--rpc".to_string(),
            "10.0.0.29:50052".to_string(),
            "-ts".to_string(),
            "3.9990,0.1000".to_string(),
        ];
        assert_eq!(NativeEngineClient::model_ready_timeout_secs(&args), 600);
    }

    #[test]
    fn model_ready_timeout_env_override_applies_regardless_of_rpc() {
        let _guard = env_lock().lock().expect("env lock poisoned");
        std::env::set_var("GHOSTLINK_MODEL_READY_TIMEOUT_SECS", "45");

        let single_node_args: Vec<String> = vec![];
        assert_eq!(
            NativeEngineClient::model_ready_timeout_secs(&single_node_args),
            45
        );

        let distributed_args = vec!["--rpc".to_string(), "host:1".to_string()];
        assert_eq!(
            NativeEngineClient::model_ready_timeout_secs(&distributed_args),
            45
        );

        std::env::remove_var("GHOSTLINK_MODEL_READY_TIMEOUT_SECS");
    }

    #[test]
    fn model_ready_timeout_ignores_invalid_or_zero_env_override() {
        let _guard = env_lock().lock().expect("env lock poisoned");
        std::env::set_var("GHOSTLINK_MODEL_READY_TIMEOUT_SECS", "not-a-number");
        let single_node_args: Vec<String> = vec![];
        assert_eq!(
            NativeEngineClient::model_ready_timeout_secs(&single_node_args),
            90
        );

        std::env::set_var("GHOSTLINK_MODEL_READY_TIMEOUT_SECS", "0");
        assert_eq!(
            NativeEngineClient::model_ready_timeout_secs(&single_node_args),
            90
        );

        std::env::remove_var("GHOSTLINK_MODEL_READY_TIMEOUT_SECS");
    }

    #[test]
    #[ignore] // requires a live llama-server; run manually with --ignored
    fn generate_chat_stream_yields_incremental_chunks_against_live_server() {
        use futures::StreamExt;
        let _guard = env_lock().lock().expect("env lock poisoned");
        std::env::set_var("GHOSTLINK_LLAMA_SERVER_URL", "http://127.0.0.1:8080");

        let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
        let engine = NativeEngineClient::new();
        rt.block_on(async {
            let mut stream = engine
                .generate_chat_stream(
                    "test-model",
                    "Count from one to five, one number per line.",
                    64,
                    0.7,
                    0.9,
                    40,
                    1.1,
                    None,
                    false,
                )
                .await
                .expect("stream should start");

            let mut chunk_count = 0usize;
            let mut chunk_times = Vec::new();
            let start = std::time::Instant::now();
            let mut full_text = String::new();
            while let Some(item) = stream.next().await {
                let text = item.expect("chunk should not error");
                chunk_count += 1;
                chunk_times.push(start.elapsed());
                full_text.push_str(&text);
            }
            eprintln!("received {chunk_count} chunks over {:?}", start.elapsed());
            eprintln!("first few chunk arrival times: {:?}", &chunk_times[..chunk_times.len().min(5)]);
            eprintln!("accumulated text: {full_text:?}");
            assert!(
                chunk_count > 1,
                "expected multiple incremental chunks, got {chunk_count} (looks like buffering, not streaming)"
            );
        });

        std::env::remove_var("GHOSTLINK_LLAMA_SERVER_URL");
    }

    #[test]
    fn native_engine_generates_preview() {
        let _guard = env_lock().lock().expect("env lock poisoned");
        std::env::remove_var("GHOSTLINK_NATIVE_ENGINE");
        std::env::remove_var("GHOSTLINK_MODEL_PATH");

        let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
        let engine = NativeEngineClient::new();
        let out = rt
            .block_on(async {
                engine
                    .generate(
                        "ghostlink-30b-v1",
                        "summarize distributed runtime scheduling",
                        128,
                        0.7,
                        0.9,
                        40,
                        1.1,
                        "simulated",
                        &[],
                        None,
                        false,
                        None,
                    )
                    .await
            })
            .expect("native generation should succeed");
        assert!(out.text.contains("[native:ghostlink-30b-v1]"));
        assert!(out.text.contains("token budget 128"));
        assert!(!out.real_inference);
    }

    /// Reads one HTTP/1.1 request off `stream` (headers, then the exact
    /// `Content-Length` body bytes) and returns the body as a string. No
    /// mocking crate — a real socket, real bytes, matching how this
    /// project's other real-TCP tests already work (see
    /// ghostlink-core::runtime's stage-worker round-trip test).
    fn read_http_request_body(stream: &mut std::net::TcpStream) -> String {
        use std::io::{BufRead, BufReader, Read};
        let mut reader = BufReader::new(stream);
        let mut content_length = 0usize;
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).expect("read header line");
            if line == "\r\n" || line.is_empty() {
                break;
            }
            if let Some(value) = line
                .to_ascii_lowercase()
                .strip_prefix("content-length:")
                .map(str::trim)
            {
                content_length = value.parse().unwrap_or(0);
            }
        }
        let mut body = vec![0u8; content_length];
        reader.read_exact(&mut body).expect("read request body");
        String::from_utf8(body).expect("request body should be utf8")
    }

    #[test]
    fn generate_sends_the_requested_id_slot_and_cache_prompt_to_llama_server() {
        let _guard = env_lock().lock().expect("env lock poisoned");

        let listener =
            std::net::TcpListener::bind("127.0.0.1:0").expect("failed to bind test listener");
        let addr = listener.local_addr().expect("failed to read local_addr");

        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept coordinator connection");
            let body = read_http_request_body(&mut stream);
            let response_body = serde_json::json!({
                "choices": [{"message": {"content": "hello from the test server"}}]
            })
            .to_string();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            use std::io::Write;
            stream
                .write_all(response.as_bytes())
                .expect("write mock response");
            body
        });

        std::env::set_var("GHOSTLINK_LLAMA_SERVER_URL", format!("http://{addr}"));

        let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
        let engine = NativeEngineClient::new();
        let out = rt
            .block_on(async {
                engine
                    .generate(
                        "test-model",
                        "hello",
                        32,
                        0.7,
                        0.9,
                        40,
                        1.1,
                        "llama_server",
                        &[],
                        Some(0),
                        true,
                        None,
                    )
                    .await
            })
            .expect("generation against the test server should succeed");
        assert_eq!(out.text, "hello from the test server");

        let captured_body = server.join().expect("server thread should not panic");
        let parsed: serde_json::Value =
            serde_json::from_str(&captured_body).expect("captured body should be valid JSON");
        assert_eq!(
            parsed.get("id_slot").and_then(|v| v.as_i64()),
            Some(0),
            "expected id_slot: 0 in the outgoing request, got: {captured_body}"
        );
        assert_eq!(
            parsed.get("cache_prompt").and_then(|v| v.as_bool()),
            Some(true),
            "expected cache_prompt: true in the outgoing request, got: {captured_body}"
        );

        std::env::remove_var("GHOSTLINK_LLAMA_SERVER_URL");
    }

    #[test]
    fn generate_weaves_prior_turns_into_the_messages_array() {
        let _guard = env_lock().lock().expect("env lock poisoned");

        let listener =
            std::net::TcpListener::bind("127.0.0.1:0").expect("failed to bind test listener");
        let addr = listener.local_addr().expect("failed to read local_addr");

        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept coordinator connection");
            let body = read_http_request_body(&mut stream);
            let response_body = serde_json::json!({
                "choices": [{"message": {"content": "hello again"}}]
            })
            .to_string();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            use std::io::Write;
            stream
                .write_all(response.as_bytes())
                .expect("write mock response");
            body
        });

        std::env::set_var("GHOSTLINK_LLAMA_SERVER_URL", format!("http://{addr}"));

        let history = vec![
            (
                "user".to_string(),
                "what's the capital of France?".to_string(),
            ),
            ("assistant".to_string(), "Paris.".to_string()),
        ];

        let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
        let engine = NativeEngineClient::new();
        rt.block_on(async {
            engine
                .generate(
                    "test-model",
                    "and its population?",
                    32,
                    0.7,
                    0.9,
                    40,
                    1.1,
                    "llama_server",
                    &history,
                    None,
                    false,
                    None,
                )
                .await
        })
        .expect("generation against the test server should succeed");

        let captured_body = server.join().expect("server thread should not panic");
        let parsed: serde_json::Value =
            serde_json::from_str(&captured_body).expect("captured body should be valid JSON");
        let messages = parsed
            .get("messages")
            .and_then(|v| v.as_array())
            .expect("messages array");

        // system, then the two history turns in order, then the new user turn.
        assert_eq!(
            messages.len(),
            4,
            "unexpected messages shape: {captured_body}"
        );
        assert_eq!(messages[0]["role"], "system");
        assert_eq!(messages[1]["role"], "user");
        assert_eq!(messages[1]["content"], "what's the capital of France?");
        assert_eq!(messages[2]["role"], "assistant");
        assert_eq!(messages[2]["content"], "Paris.");
        assert_eq!(messages[3]["role"], "user");
        assert_eq!(messages[3]["content"], "and its population?");

        std::env::remove_var("GHOSTLINK_LLAMA_SERVER_URL");
    }

    #[test]
    fn llama_mode_requires_model_path() {
        let _guard = env_lock().lock().expect("env lock poisoned");

        let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
        let engine = NativeEngineClient::new();
        std::env::set_var("GHOSTLINK_NATIVE_ENGINE", "llama_cpp");
        std::env::remove_var("GHOSTLINK_MODEL_PATH");
        let err = rt
            .block_on(async {
                engine
                    .generate(
                        "ghostlink-30b-v1",
                        "hello",
                        32,
                        0.7,
                        0.9,
                        40,
                        1.1,
                        "llama_cpp",
                        &[],
                        None,
                        false,
                        None,
                    )
                    .await
            })
            .expect_err("llama mode without model path should fail");
        assert!(err.contains("GHOSTLINK_MODEL_PATH"));
        std::env::remove_var("GHOSTLINK_NATIVE_ENGINE");
    }

    #[test]
    fn get_ngl_tiers_taper_down_with_less_vram() {
        let _guard = env_lock().lock().expect("env lock poisoned");
        std::env::remove_var("GHOSTLINK_LLAMA_NGL");

        // Small model (well under the 10GB large-model cap) so this test
        // exercises only the VRAM tiering, unaffected by model size.
        let case = |vram: &str| {
            std::env::set_var("GHOSTLINK_VRAM_GB", vram);
            let n = NativeEngineClient::get_ngl(1.0);
            std::env::remove_var("GHOSTLINK_VRAM_GB");
            n
        };

        // Regression: the <8GB tier previously returned 99 (llama.cpp's
        // "offload every layer" sentinel) instead of a smaller value,
        // which would OOM low-VRAM cards instead of degrading safely.
        assert_eq!(case("16"), 40);
        assert_eq!(case("12"), 40);
        assert_eq!(case("10"), 24);
        assert_eq!(case("8"), 24);
        assert_eq!(case("6"), 12);
        assert_eq!(case("4"), 12);
        assert_eq!(case("2"), 0);

        // Values must never increase as VRAM decreases.
        let tiers = [16.0, 12.0, 10.0, 8.0, 6.0, 4.0, 2.0];
        let mut last = i32::MAX;
        for vram in tiers {
            let n = case(&vram.to_string());
            assert!(
                n <= last,
                "ngl must not increase as VRAM decreases: {vram}GB -> {n}, previous tier -> {last}"
            );
            last = n;
        }

        // GHOSTLINK_LLAMA_NGL takes priority over auto-detect from VRAM,
        // even for a large model that would otherwise be capped to 0.
        std::env::set_var("GHOSTLINK_LLAMA_NGL", "7");
        std::env::set_var("GHOSTLINK_VRAM_GB", "16");
        assert_eq!(NativeEngineClient::get_ngl(20.0), 7);
        std::env::remove_var("GHOSTLINK_LLAMA_NGL");
        std::env::remove_var("GHOSTLINK_VRAM_GB");
    }

    #[test]
    fn get_ngl_caps_large_models_to_cpu_only_regardless_of_vram_tier() {
        let _guard = env_lock().lock().expect("env lock poisoned");
        std::env::remove_var("GHOSTLINK_LLAMA_NGL");

        // Measured directly on the reference hardware (AMD Radeon 860M,
        // Vulkan backend, 27.6GB host): loading a 13.6GB model cost ~0.54GB
        // committed memory CPU-only vs ~14.15GB at full offload, for only
        // 6.48 -> ~18 tok/s — a 26x memory cost for well under 3x speed,
        // and the full-offload case left under 1GB free RAM. A generous
        // VRAM tier must not undo that safety margin for a large model.
        std::env::set_var("GHOSTLINK_VRAM_GB", "16");
        assert_eq!(NativeEngineClient::get_ngl(13.6), 0);
        assert_eq!(NativeEngineClient::get_ngl(10.0), 0);
        // Just under the cap: ordinary VRAM tiering applies.
        assert_eq!(NativeEngineClient::get_ngl(9.9), 40);
        std::env::remove_var("GHOSTLINK_VRAM_GB");

        // Same cap applies with no VRAM env var set (the "-1, let
        // llama-server decide" default) — that default meant "offload
        // everything" before this fix, which is exactly the dangerous case.
        assert_eq!(NativeEngineClient::get_ngl(13.6), 0);
    }

    #[test]
    fn get_ctx_size_is_capped_down_for_large_models_regardless_of_vram_tier() {
        let _guard = env_lock().lock().expect("env lock poisoned");
        std::env::remove_var("GHOSTLINK_CTX_SIZE");
        std::env::set_var("GHOSTLINK_VRAM_GB", "8");

        // Regression: a 13.6GB model + the ">=8GB" VRAM tier's nominal 8192
        // ctx used to combine with model weights to leave a 27.6GB host
        // under 1GB free RAM. Large models must get a much smaller ceiling
        // than the VRAM tier alone would grant, since KV cache and model
        // weights share the same finite memory on a unified-memory iGPU.
        assert_eq!(NativeEngineClient::get_ctx_size(13.6), 4096);
        assert_eq!(NativeEngineClient::get_ctx_size(10.0), 4096);
        // Mid-size: capped, but less aggressively.
        assert_eq!(NativeEngineClient::get_ctx_size(7.0), 8192);
        assert_eq!(NativeEngineClient::get_ctx_size(5.0), 8192);
        // Small model: uncapped, gets the full VRAM-tier ceiling.
        assert_eq!(NativeEngineClient::get_ctx_size(0.6), 8192);

        std::env::remove_var("GHOSTLINK_VRAM_GB");
    }

    #[test]
    fn get_ctx_size_explicit_override_wins_regardless_of_model_size() {
        let _guard = env_lock().lock().expect("env lock poisoned");
        std::env::set_var("GHOSTLINK_CTX_SIZE", "32768");
        std::env::set_var("GHOSTLINK_VRAM_GB", "4");

        // GHOSTLINK_CTX_SIZE is an explicit "I know what I'm doing" override
        // — it must win even for a large model that would otherwise be capped.
        assert_eq!(NativeEngineClient::get_ctx_size(20.0), 32768);

        std::env::remove_var("GHOSTLINK_CTX_SIZE");
        std::env::remove_var("GHOSTLINK_VRAM_GB");
    }

    #[test]
    fn get_ctx_size_defaults_to_8192_without_vram_env_var() {
        let _guard = env_lock().lock().expect("env lock poisoned");
        std::env::remove_var("GHOSTLINK_CTX_SIZE");
        std::env::remove_var("GHOSTLINK_VRAM_GB");

        assert_eq!(NativeEngineClient::get_ctx_size(0.6), 8192);
        assert_eq!(NativeEngineClient::get_ctx_size(13.6), 4096);
    }

    #[test]
    fn get_parallel_slots_defaults_to_one_when_unset() {
        let _guard = env_lock().lock().expect("env lock poisoned");
        std::env::remove_var("GHOSTLINK_PARALLEL_SLOTS");
        assert_eq!(NativeEngineClient::get_parallel_slots(), 1);
    }

    #[test]
    fn get_parallel_slots_reads_env_and_clamps_range() {
        let _guard = env_lock().lock().expect("env lock poisoned");

        std::env::set_var("GHOSTLINK_PARALLEL_SLOTS", "4");
        assert_eq!(NativeEngineClient::get_parallel_slots(), 4);

        // Clamped, not rejected: 0 would make llama-server refuse to start.
        std::env::set_var("GHOSTLINK_PARALLEL_SLOTS", "0");
        assert_eq!(NativeEngineClient::get_parallel_slots(), 1);

        std::env::set_var("GHOSTLINK_PARALLEL_SLOTS", "9999");
        assert_eq!(NativeEngineClient::get_parallel_slots(), 64);

        // Unparseable input falls back to the safe default rather than panicking.
        std::env::set_var("GHOSTLINK_PARALLEL_SLOTS", "not-a-number");
        assert_eq!(NativeEngineClient::get_parallel_slots(), 1);

        std::env::remove_var("GHOSTLINK_PARALLEL_SLOTS");
    }

    #[test]
    fn get_mlock_explicit_override_wins_regardless_of_ram() {
        let _guard = env_lock().lock().expect("env lock poisoned");

        std::env::set_var("GHOSTLINK_MLOCK", "1");
        assert!(NativeEngineClient::get_mlock());

        std::env::set_var("GHOSTLINK_MLOCK", "0");
        assert!(!NativeEngineClient::get_mlock());

        std::env::remove_var("GHOSTLINK_MLOCK");
    }

    #[test]
    fn get_mlock_defaults_off_below_24gb_ram() {
        let _guard = env_lock().lock().expect("env lock poisoned");
        std::env::remove_var("GHOSTLINK_MLOCK");
        std::env::set_var("GHOSTLINK_SYSTEM_MEMORY_GB", "16");

        // Mirrors launch.sh: mlock is a real risk on a memory-tight host
        // (pinning model pages can starve everything else instead of just
        // risking swap), so it must stay off by default there.
        assert!(!NativeEngineClient::get_mlock());

        std::env::remove_var("GHOSTLINK_SYSTEM_MEMORY_GB");
    }

    #[test]
    fn get_mlock_defaults_on_at_24gb_ram_and_above() {
        let _guard = env_lock().lock().expect("env lock poisoned");
        std::env::remove_var("GHOSTLINK_MLOCK");
        std::env::set_var("GHOSTLINK_SYSTEM_MEMORY_GB", "32");

        assert!(NativeEngineClient::get_mlock());

        std::env::remove_var("GHOSTLINK_SYSTEM_MEMORY_GB");
    }

    #[test]
    fn get_no_mmap_is_opt_in_only() {
        let _guard = env_lock().lock().expect("env lock poisoned");

        std::env::remove_var("GHOSTLINK_NO_MMAP");
        assert!(!NativeEngineClient::get_no_mmap());

        std::env::set_var("GHOSTLINK_NO_MMAP", "1");
        assert!(NativeEngineClient::get_no_mmap());

        std::env::set_var("GHOSTLINK_NO_MMAP", "0");
        assert!(!NativeEngineClient::get_no_mmap());

        std::env::remove_var("GHOSTLINK_NO_MMAP");
    }

    #[test]
    fn get_batch_ubatch_tiers_with_vram_and_never_increase_as_vram_decreases() {
        let _guard = env_lock().lock().expect("env lock poisoned");
        std::env::remove_var("GHOSTLINK_LLAMA_BATCH");
        std::env::remove_var("GHOSTLINK_LLAMA_UBATCH");

        let case = |vram: &str| {
            std::env::set_var("GHOSTLINK_VRAM_GB", vram);
            let r = NativeEngineClient::get_batch_ubatch();
            std::env::remove_var("GHOSTLINK_VRAM_GB");
            r
        };

        assert_eq!(case("16"), (2048, 512));
        assert_eq!(case("12"), (2048, 512));
        assert_eq!(case("8"), (1024, 512));
        assert_eq!(case("4"), (512, 256));
        assert_eq!(case("2"), (512, 128));

        let tiers = [16.0, 12.0, 8.0, 4.0, 2.0];
        let mut last = (u32::MAX, u32::MAX);
        for vram in tiers {
            let (b, ub) = case(&vram.to_string());
            assert!(
                b <= last.0 && ub <= last.1,
                "batch/ubatch must not increase as VRAM decreases: {vram}GB -> ({b},{ub}), previous -> {last:?}"
            );
            last = (b, ub);
        }
    }

    #[test]
    fn get_batch_ubatch_explicit_overrides_are_independent() {
        let _guard = env_lock().lock().expect("env lock poisoned");
        std::env::set_var("GHOSTLINK_VRAM_GB", "16");

        // Only batch overridden -- ubatch still comes from the VRAM tier.
        std::env::set_var("GHOSTLINK_LLAMA_BATCH", "4096");
        std::env::remove_var("GHOSTLINK_LLAMA_UBATCH");
        assert_eq!(NativeEngineClient::get_batch_ubatch(), (4096, 512));

        // Only ubatch overridden -- batch still comes from the VRAM tier.
        std::env::remove_var("GHOSTLINK_LLAMA_BATCH");
        std::env::set_var("GHOSTLINK_LLAMA_UBATCH", "1024");
        assert_eq!(NativeEngineClient::get_batch_ubatch(), (2048, 1024));

        // Zero/unparseable overrides fall through to the tier default rather
        // than producing a `-b 0` llama-server would reject.
        std::env::set_var("GHOSTLINK_LLAMA_BATCH", "0");
        std::env::set_var("GHOSTLINK_LLAMA_UBATCH", "not-a-number");
        assert_eq!(NativeEngineClient::get_batch_ubatch(), (2048, 512));

        std::env::remove_var("GHOSTLINK_LLAMA_BATCH");
        std::env::remove_var("GHOSTLINK_LLAMA_UBATCH");
        std::env::remove_var("GHOSTLINK_VRAM_GB");
    }

    #[test]
    fn get_kv_cache_type_defaults_to_q8_0_and_rejects_unknown_values() {
        let _guard = env_lock().lock().expect("env lock poisoned");

        std::env::remove_var("GHOSTLINK_LLAMA_KV_CACHE_TYPE");
        assert_eq!(NativeEngineClient::get_kv_cache_type(), "q8_0");

        std::env::set_var("GHOSTLINK_LLAMA_KV_CACHE_TYPE", "f16");
        assert_eq!(NativeEngineClient::get_kv_cache_type(), "f16");

        std::env::set_var("GHOSTLINK_LLAMA_KV_CACHE_TYPE", "q4_0");
        assert_eq!(NativeEngineClient::get_kv_cache_type(), "q4_0");

        // Unknown/typo'd value falls back to the safe default instead of
        // passing something llama-server might reject outright.
        std::env::set_var("GHOSTLINK_LLAMA_KV_CACHE_TYPE", "q99_bogus");
        assert_eq!(NativeEngineClient::get_kv_cache_type(), "q8_0");

        std::env::remove_var("GHOSTLINK_LLAMA_KV_CACHE_TYPE");
    }

    #[test]
    fn get_flash_attention_defaults_on_matching_prior_unconditional_behavior() {
        let _guard = env_lock().lock().expect("env lock poisoned");

        std::env::remove_var("GHOSTLINK_LLAMA_FLASH_ATTN");
        assert!(NativeEngineClient::get_flash_attention());

        std::env::set_var("GHOSTLINK_LLAMA_FLASH_ATTN", "0");
        assert!(!NativeEngineClient::get_flash_attention());

        std::env::set_var("GHOSTLINK_LLAMA_FLASH_ATTN", "off");
        assert!(!NativeEngineClient::get_flash_attention());

        // Anything else (including a typo) stays on the safe/prior default.
        std::env::set_var("GHOSTLINK_LLAMA_FLASH_ATTN", "nope");
        assert!(NativeEngineClient::get_flash_attention());

        std::env::remove_var("GHOSTLINK_LLAMA_FLASH_ATTN");
    }

    #[test]
    fn default_perf_args_omits_kv_cache_flags_when_flash_attention_is_off() {
        let _guard = env_lock().lock().expect("env lock poisoned");
        std::env::remove_var("GHOSTLINK_LLAMA_SERVER_ARGS");
        std::env::set_var("GHOSTLINK_LLAMA_FLASH_ATTN", "0");

        // Regression: llama.cpp requires Flash Attention for quantized KV
        // cache -- passing -ctk/-ctv without -fa on would make the server
        // fail to load instead of silently ignoring the flags.
        let args = NativeEngineClient::get_llama_server_args();
        assert!(!args.contains(&"-fa".to_string()));
        assert!(!args.contains(&"-ctk".to_string()));
        assert!(!args.contains(&"-ctv".to_string()));
        assert!(args.contains(&"-b".to_string()));

        std::env::remove_var("GHOSTLINK_LLAMA_FLASH_ATTN");
    }

    #[test]
    fn has_running_llama_server_reflects_actual_process_state() {
        let _guard = env_lock().lock().expect("env lock poisoned");
        let engine = NativeEngineClient::new();

        // Regression: `has_running_llama_server` used to only check that a
        // `Child` handle existed (`c.id() > 0`, which is true for every real
        // PID), so a process that already crashed/exited but hadn't been
        // reaped yet was still reported as "running".
        let handle = NativeEngineClient::get_process_handle();

        let mut child = if cfg!(windows) {
            std::process::Command::new("cmd")
                .args(["/C", "exit 0"])
                .spawn()
        } else {
            std::process::Command::new("true").spawn()
        }
        .expect("spawn short-lived helper process");

        // Block until the helper process has actually exited instead of
        // sleeping a fixed duration and hoping it's done by then. Under
        // system load (e.g. `cargo test --workspace` running many tests
        // concurrently) a fixed sleep isn't reliably long enough, and a
        // panic here while holding `_guard` poisons `env_lock()` for every
        // other test in this file that also locks it. `wait()` blocks for
        // as long as it actually takes, so this is deterministic regardless
        // of scheduling delays.
        child.wait().expect("helper process should exit");

        *handle.lock().expect("process handle lock poisoned") = Some(child);

        assert!(
            !engine.has_running_llama_server(),
            "an exited process must not be reported as a running llama-server"
        );

        *handle.lock().expect("process handle lock poisoned") = None;
    }
}
