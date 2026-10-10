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

/// One item from a native chat stream: an incremental text delta, or a terminal
/// marker that the model stopped because it hit the token budget
/// (llama-server reports `finish_reason: "length"`). Surfaced the moment the
/// backend reports it so the GUI can flag truncation without waiting for the
/// final chunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NativeChatEvent {
    Delta(String),
    Truncated,
}

/// Stream of incremental events from a native backend's chat endpoint.
pub type NativeChatStream = Pin<Box<dyn Stream<Item = Result<NativeChatEvent, String>> + Send>>;

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
    /// Prompt tokens evaluated (prefill work).
    ///
    /// Kept separate from `tokens_generated` because the two costs are unrelated:
    /// prefill is compute-bound and batches well, decode is bandwidth-bound and does
    /// not. One number for both is how a 300 tok/s prefill and a 2 tok/s decode end
    /// up looking like a single fast model.
    pub prompt_tokens: Option<u32>,
    /// Prompt evaluation wall time in ms, from llama.cpp's own timings.
    pub prompt_ms: Option<f32>,
    /// Prefill throughput tok/s, derived from the two above.
    pub prompt_tokens_per_sec: Option<f32>,
}

impl NativeGeneration {
    fn text_only(text: String, real: bool) -> Self {
        Self {
            text,
            real_inference: real,
            tokens_generated: None,
            tokens_per_sec: None,
            latency_ms: None,
            prompt_tokens: None,
            prompt_ms: None,
            prompt_tokens_per_sec: None,
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

/// Context size the running llama-server was launched with.
static RUNNING_CTX: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// P-core-only thread count for `-t`, or `None` when the CPU is not hybrid.
///
/// Returns `None` rather than a guess when detection is unavailable or reports no
/// split, so the caller falls back to `available_parallelism` unchanged. Silently
/// halving threads on a uniform-core CPU would be a large, invisible regression.
fn hybrid_performance_threads(logical: usize) -> Option<usize> {
    let profile = ghostlink_core::system_profile::SystemProfile::detect_fast();
    let performance = profile.cpu.performance_cores?;
    if performance == 0 {
        return None;
    }
    // `performance` is a physical P-core count while `logical` is every logical
    // processor. Scaling keeps SMT threads on the performance cores; limiting to
    // physical P-cores alone would leave half the FP throughput unused.
    let physical = profile.cpu.physical_cores.max(performance);
    let scaled = if physical > 0 {
        (performance as f64 * logical as f64 / physical as f64).round() as usize
    } else {
        performance
    };
    // Never exceed the logical count, and never fall below half of it: if the reported
    // numbers are inconsistent, a smaller pool is still better than an unusable one.
    Some(scaled.clamp(logical / 2, logical))
}

/// A non-default tuning value present in `settings.json` while its `*_auto` flag is true.
///
/// Returns a note, or `None` when there is nothing to report. Reads the settings file
/// rather than taking it as an argument, so the diagnostic cannot be handed a stale copy
/// of what the user configured.
fn auto_ignored_setting(label: &str) -> Option<String> {
    auto_ignored_setting_in(label, std::path::Path::new("settings.json"))
}

/// `auto_ignored_setting` against an explicit settings path.
///
/// Split out so tests can point at a fixture. Reading a fixed relative path instead
/// would mean `std::env::set_current_dir`, which is process-global and breaks every
/// other test in this binary that reads a relative path.
fn auto_ignored_setting_in(label: &str, settings_path: &std::path::Path) -> Option<String> {
    let (field, auto_field, default) = match label {
        "ctx_size" => ("ctx_size", "ctx_size_auto", 8192i64),
        "ngl" => ("ngl", "ngl_auto", -1i64),
        _ => ("threads", "threads_auto", 4i64),
    };
    let Ok(text) = std::fs::read_to_string(settings_path) else {
        return None;
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
        return None;
    };
    // An absent `*_auto` deserializes as true, which is exactly the case that matters.
    let auto = v.get(auto_field).and_then(|b| b.as_bool()).unwrap_or(true);
    if !auto {
        return None;
    }
    let requested = v.get(field).and_then(|n| n.as_i64())?;
    if requested == default {
        return None;
    }
    Some(format!("{field}={requested}"))
}

/// The tuning actually applied to a model load, and anything the policy overrode.
///
/// Added because configured values were being discarded in silence. Measured on this
/// machine: `settings.json` carries `ctx_size: 131072` and `ngl: 100`, both with
/// `*_auto: true`, and `llama-server` was launched:
///
/// ```text
/// -m models/Qwen3.8-27B-UD-IQ3_S.gguf -c 4096 -np 1 -ngl 0 -t 15
/// ```
///
/// The overrides are defensible -- a 12 GB model CPU-bound beats a partial offload that
/// OOMs, which is measured -- but *silently* is not the same as *correctly*, and a user
/// reading `settings.json` had no way to tell.
///
/// Two failure modes are reported separately because the fixes differ:
///
/// * **OVERRIDDEN** -- the value reached the environment and something else won.
/// * **IGNORED** -- `*_auto` is true so the value never left `settings.json` at all. This
///   is the more misleading one: the file shows a number that is not used anywhere.
pub fn describe_tuning(model_size_gb: f32) -> String {
    let ctx = NativeEngineClient::get_ctx_size(model_size_gb);
    let ngl = NativeEngineClient::get_ngl(model_size_gb);
    let threads = NativeEngineClient::get_threads();
    let mut out = format!(
        "[perf-tier] effective: -c {ctx} -ngl {ngl} -t {threads} (model {model_size_gb:.2} GB)"
    );

    let mut ignored: Vec<String> = Vec::new();
    let mut overridden: Vec<String> = Vec::new();
    for (env, label) in [
        ("GHOSTLINK_CTX_SIZE", "ctx_size"),
        ("GHOSTLINK_LLAMA_NGL", "ngl"),
        ("GHOSTLINK_LLAMA_THREADS", "threads"),
    ] {
        let effective = match env {
            "GHOSTLINK_CTX_SIZE" => ctx.to_string(),
            "GHOSTLINK_LLAMA_NGL" => ngl.to_string(),
            _ => threads.to_string(),
        };
        match std::env::var(env) {
            Ok(requested) => {
                let requested = requested.trim().to_string();
                if requested != effective {
                    overridden.push(format!("{label}: requested {requested}, using {effective}"));
                }
            }
            Err(_) => {
                if let Some(field) = auto_ignored_setting(label) {
                    ignored.push(field);
                }
            }
        }
    }

    if !overridden.is_empty() {
        out.push_str("\n[perf-tier] OVERRIDDEN by model-size/VRAM policy: ");
        out.push_str(&overridden.join("; "));
        out.push_str("\n[perf-tier] clear the matching *_auto flag to force the setting");
    }
    if !ignored.is_empty() {
        out.push_str("\n[perf-tier] IGNORED in settings.json (*_auto is true): ");
        out.push_str(&ignored.join("; "));
        out.push_str(
            "\n[perf-tier] these look configured but are never used; clear the matching \
             *_auto flag to apply them",
        );
    }
    out
}

impl NativeEngineClient {
    pub fn new() -> Self {
        Self {
            http: reqwest::Client::builder()
                .pool_max_idle_per_host(10)
                .tcp_keepalive(std::time::Duration::from_secs(15))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
        }
    }

    /// Static system prompt for both the non-streaming (`generate_with_llama_server`)
    /// and streaming (`generate_chat_stream`) chat paths. Small local models otherwise
    /// default to run-on, ungrammatical prose with no Markdown structure — spelling out
    /// formatting expectations here measurably improves readability since these models
    /// have no other source of style guidance (no fine-tuning on Ghostlink's own output).
    ///
    /// **Must stay byte-identical across requests.** llama-server's `cache_prompt`
    /// (prefix cache) only reuses a slot's KV state for a common prefix; any
    /// per-request byte in this string — a timestamp, a request id — changes the
    /// very first token and forces a full re-prefill of the entire conversation
    /// on every turn. Everything genuinely dynamic therefore lives in
    /// `dynamic_context_suffix()` and is appended to the *user* turn instead.
    fn static_system_prompt() -> String {
        "You are a helpful, precise assistant. Write clear, grammatically correct \
         responses in clean Markdown: headings (#, ##, ###) on their own line with \
         blank lines around them, lists for multiple items, **bold** labels before \
         values, and fenced code blocks with a language tag. Keep prose tight — no \
         run-on paragraphs."
            .to_string()
    }

    /// Per-request dynamic context, appended to the user turn rather than the
    /// system prompt so the cached system prefix stays stable (see
    /// `static_system_prompt`). Models have no clock; this is what lets
    /// questions like "what date is it today?" get a correct answer.
    /// Portable chrono format (avoid %-d, which is Unix-only).
    pub(crate) fn dynamic_context_suffix() -> String {
        format!(
            "[Context: current local date and time is {}]",
            chrono::Local::now().format("%A, %B %d, %Y, %H:%M")
        )
    }

    /// The user-turn content sent to a chat endpoint: the caller's prompt with
    /// the dynamic context appended. Kept as one helper so every backend path
    /// splits static/dynamic identically — Ollama/vLLM builders in `main.rs`
    /// call this too, so the timestamp lives outside every cached prefix.
    pub(crate) fn user_turn_with_context(cleaned_prompt: &str) -> String {
        format!("{cleaned_prompt}\n\n{}", Self::dynamic_context_suffix())
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
    /// Tokenize content using llama-server's /tokenize endpoint for accurate token counts.
    pub async fn tokenize(&self, content: &str) -> Option<usize> {
        let base_url = Self::get_llama_base_url();
        let url = format!("{base_url}/tokenize");
        let payload = serde_json::json!({ "content": content });
        let response = self.http.post(&url).json(&payload).send().await.ok()?;
        if response.status().is_success() {
            let json: serde_json::Value = response.json().await.ok()?;
            if let Some(tokens) = json.get("tokens").and_then(|v| v.as_array()) {
                return Some(tokens.len().max(1));
            }
        }
        None
    }

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
    ///
    /// Public so the chat path can clamp `conversation_token_limit` against the
    /// context the model is actually running with. The two can disagree -- the
    /// setting is a user preference while this is derived from VRAM and model size
    /// -- and when they do, budgeting against the setting produces prompts the model
    /// cannot accept at all. See `history_budget_tokens`.
    /// The context size the currently-running llama-server was launched with, or 0
    /// when unknown (no server started yet, or it was started by something else).
    ///
    /// 0 rather than a guessed default: a caller that clamps against an invented
    /// number would silently under- or over-budget, and there is already a ceiling
    /// (`GHOSTLINK_CTX_SIZE`) that makes the real value discoverable.
    pub(crate) fn running_ctx_size() -> u32 {
        RUNNING_CTX.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn record_running_ctx(ctx: u32) {
        RUNNING_CTX.store(ctx, std::sync::atomic::Ordering::Relaxed);
    }

    /// The context size the **running** llama-server reports, from `/props`.
    ///
    /// Preferred over the value derived from VRAM and model size because it is the
    /// server's own answer rather than our prediction of it. It also covers the case
    /// that made the derived value useless: a server Ghostlink did not launch. If the
    /// user has one already running on the configured port, `load_model` reuses it and
    /// never calls `get_ctx_size`, so the recorded value stayed 0 and the history
    /// budget fell back to `conversation_token_limit` -- which is why the live 80-turn
    /// request was still trimmed to 18,851 tokens against a real 8192 ctx.
    ///
    /// Best effort: any failure returns 0, and callers treat 0 as "unknown" and keep
    /// the previous ceiling rather than guessing.
    /// Whether the configured `llama-server` binary was built with RPC support.
    ///
    /// Verified rather than assumed, because a binary built without `GGML_RPC` rejects
    /// `--rpc` outright. Measured against this repo's vendored build:
    ///
    /// ```text
    /// $ llama-server --version
    /// version 0.5.0-dev (build 1, commit 4b1a27f)
    /// $ llama-server ... --rpc 127.0.0.1:59999
    /// error: invalid argument: --rpc
    /// $ grep GGML_RPC build/CMakeCache.txt
    /// GGML_RPC:BOOL=OFF
    /// ```
    ///
    /// So `--rpc` and `-ts` were being handed to a process that exits 1 on them. One
    /// probe at launch, cached: cheap, and it turns a silent load failure into a
    /// specific, actionable message.
    pub(crate) fn binary_supports_rpc() -> bool {
        use std::sync::OnceLock;
        static SUPPORTS: OnceLock<bool> = OnceLock::new();
        *SUPPORTS.get_or_init(|| {
            let bin = Self::get_llama_server_bin();
            if bin == "llama-server" || bin == "llama-server.exe" || !Path::new(&bin).exists() {
                return false;
            }
            match std::process::Command::new(&bin).arg("--help").output() {
                Ok(out) => {
                    let text = format!(
                        "{}{}",
                        String::from_utf8_lossy(&out.stdout),
                        String::from_utf8_lossy(&out.stderr)
                    );
                    // `--rpc` appears in the flag parser's output only when GGML_RPC is on.
                    text.contains("--rpc")
                }
                Err(_) => false,
            }
        })
    }

    pub(crate) async fn probe_running_ctx_size() -> u32 {
        let base = Self::get_llama_base_url();
        let url = format!("{}/props", base.trim_end_matches('/'));
        let client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(3))
            .build()
        {
            Ok(c) => c,
            Err(_) => return 0,
        };
        let resp = match client.get(&url).send().await {
            Ok(r) => r,
            Err(_) => return 0,
        };
        let body: serde_json::Value = match resp.json().await {
            Ok(v) => v,
            Err(_) => return 0,
        };
        // `n_ctx` is the TOTAL across all slots, not the per-request allowance. With
        // `-np 2` llama-server serves 8192 total as two 4096 contexts, and a request
        // that budgets against 8192 is rejected at ~4096. Measured: a prompt trimmed
        // to a 7168 budget came back as
        // "request (10589 tokens) exceeds the available context size".
        let n_ctx = body
            .get("default_generation_settings")
            .and_then(|s| s.get("n_ctx"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        // total_slots defaults to 1 when absent; never let a bad value divide to zero.
        let slots = body
            .get("total_slots")
            .and_then(|v| v.as_u64())
            .unwrap_or(1)
            .max(1);
        if n_ctx == 0 {
            return 0;
        }
        let n = (n_ctx / slots) as u32;
        if n == 0 {
            return 0;
        }
        Self::record_running_ctx(n);
        n
    }

    pub(crate) fn get_ctx_size(model_size_gb: f32) -> u32 {
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
    /// Speculative-decoding flags, empty when it is not configured.
    ///
    /// `GHOSTLINK_DRAFT_MODEL` is documented in `docs/LOCAL_INFERENCE_TUNING.md` with a
    /// specific promise -- "Ghostlink passes `--model-draft <path>`, `--draft-max`, and
    /// `--draft-p-min` to `llama-server`" -- and no code anywhere in this repository read
    /// that variable. Setting it did nothing at all.
    ///
    /// The documented flag names are also stale for this build. Verified against the
    /// vendored `llama-server --help`:
    ///
    /// ```text
    /// --draft, --draft-n, --draft-max N   the argument has been removed.
    ///                                   use --spec-draft-n-max or ...
    /// ```
    ///
    /// So `--draft-max` as documented would be rejected by the binary outright. These are
    /// the names this build parses.
    ///
    /// Off by default: a draft model close in size to the target costs more than it
    /// saves, and there is no reliable way to pick the pairing at runtime.
    fn draft_model_args() -> Vec<String> {
        let mut args = Vec::new();
        let Ok(path) = std::env::var("GHOSTLINK_DRAFT_MODEL") else {
            return args;
        };
        let path = path.trim().to_string();
        if path.is_empty() {
            return args;
        }
        if !Path::new(&path).exists() {
            // Logged rather than fatal: the primary model is fine and this was only an
            // optimization. Failing the load over a draft path would be strictly worse.
            eprintln!(
                "[spec-decode] GHOSTLINK_DRAFT_MODEL={path:?} does not exist; speculative \
                 decoding disabled for this load"
            );
            return args;
        }
        args.push("--spec-draft-model".to_string());
        args.push(path.clone());
        // Tokens the draft model proposes per step. Caller-set, because the right value
        // depends on the model pair and there is no defensible default.
        if let Some(n) = std::env::var("GHOSTLINK_DRAFT_MAX")
            .ok()
            .and_then(|v| v.trim().parse::<usize>().ok())
            .filter(|&n| n > 0)
        {
            args.push("--spec-draft-n-max".to_string());
            args.push(n.to_string());
        }
        if let Some(p) = std::env::var("GHOSTLINK_DRAFT_P_MIN")
            .ok()
            .and_then(|v| v.trim().parse::<f32>().ok())
            .filter(|v| v.is_finite() && *v >= 0.0)
        {
            args.push("--spec-draft-p-min".to_string());
            args.push(format!("{p}"));
        }
        eprintln!("[spec-decode] draft model {path} enabled");
        args
    }

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
        args.extend(Self::draft_model_args());
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
    /// Available system RAM and the rough amount a GPU-offloaded load of this model
    /// needs, or `None` when it cannot be determined.
    ///
    /// Returns `(available_gb, needed_gb)`. The estimate is deliberately
    /// conservative -- `needed` is the model size plus headroom for the KV cache and
    /// compute buffers, and the duplicate device allocation is folded into the
    /// headroom rather than computed exactly. Used only to warn, never to refuse.
    fn check_offload_memory_headroom(model_size_gb: f32) -> Option<(f32, f32)> {
        let ngl = Self::get_ngl(model_size_gb);
        let (needed_gb, _) = Self::check_offload_memory_headroom_for_ngl(model_size_gb, ngl)?;
        let available_gb = Self::available_system_memory_gb()?;
        Some((available_gb, needed_gb))
    }

    /// Pure part of the headroom check, split out so the arithmetic is testable
    /// without depending on live machine state.
    ///
    /// Returns `(needed_gb, model_size_gb)`, or `None` when the load is CPU-only
    /// and therefore has no duplicate allocation to make room for.
    fn check_offload_memory_headroom_for_ngl(model_size_gb: f32, ngl: i32) -> Option<(f32, f32)> {
        if ngl == 0 {
            return None;
        }
        // 1.35x covers the host copy plus a conservative share of the device copy.
        // On a unified-memory iGPU the device buffer is carved out of the same pool,
        // so this is the number that actually has to clear.
        Some((model_size_gb * 1.35, model_size_gb))
    }

    /// Whether to warn. Split out for the same reason.
    fn should_warn_about_memory(available_gb: f32, needed_gb: f32) -> bool {
        available_gb < needed_gb
    }

    /// Free physical memory in GB, via `sysinfo` (already a dependency).
    fn available_system_memory_gb() -> Option<f32> {
        use sysinfo::System;
        let mut sys = System::new();
        // This sysinfo version's `refresh_memory` returns `()`, not a Result.
        sys.refresh_memory();
        let bytes = sys.available_memory();
        if bytes == 0 {
            return None;
        }
        Some(bytes as f32 / (1024.0 * 1024.0 * 1024.0))
    }

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
        let logical = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
            .max(1);
        // On a hybrid CPU, the efficiency cores are slower at exactly the matmul work
        // llama.cpp schedules here, so handing it every logical processor lets them
        // participate in the decode loop. Measured on this host: -t 8 (P-cores plus SMT)
        // decodes 21.55 tok/s against 20.18 for -t 16, a 1.07x gain.
        //
        // Detection is Windows-only and returns None without a real P/E split, so this is
        // a no-op on uniform-core CPUs. Worth checking rather than assuming: this host is
        // a Ryzen AI 7 350, which reads as uniform-core, but
        // `GetLogicalProcessorInformationEx` reports 8 processor-core records with
        // `EfficiencyClass = [1,0,1,0,1,0,1,0]` -- 4 performance, 4 efficiency. An earlier
        // comment here asserted the opposite, and a test written from that assumption
        // failed against the real detector.
        //
        // `performance_cores` is the physical P-core count. Scaling it by the
        // logical:core ratio preserves SMT threads on the performance cores, which is what
        // the flag means in practice -- limiting to physical P-cores alone would leave
        // half the FP throughput unused.
        let threads = match hybrid_performance_threads(logical) {
            Some(p) => p,
            None => logical,
        };
        threads.max(1)
    }

    /// Number of parallel inference slots to give llama-server (`-np`).
    /// Defaults to `1` — today's exact prior behavior — unless
    /// `GHOSTLINK_PARALLEL_SLOTS` is set (mirrored from `RuntimeSettings
    /// .parallel_slots` by `load_settings()`, or set directly by a launch
    /// script). More than one slot lets llama-server serve concurrent
    /// generations instead of queueing them one at a time.
    ///
    /// This is the authoritative slot count: it is what actually shaped the
    /// running server's `-np`. Callers that need to pin a request to a slot
    /// must use *this*, not `RuntimeSettings::parallel_slots` — the persisted
    /// setting and the env var can disagree (the env var is only mirrored from
    /// settings when unset), and pinning against the wrong count silently
    /// disables pinning.
    pub(crate) fn get_parallel_slots() -> usize {
        if let Ok(val) = std::env::var("GHOSTLINK_PARALLEL_SLOTS") {
            if let Ok(n) = val.trim().parse::<usize>() {
                return n.clamp(1, 64);
            }
        }
        1
    }

    /// Stable llama-server slot for a conversation, so a returning multi-turn
    /// session lands on the slot whose KV still holds its prefix and
    /// `cache_prompt` actually hits. Without this, `id_slot: -1` (auto) lets
    /// llama-server hand any free slot to the request — with `-np > 1` a
    /// follow-up turn can be served by a slot whose cached prefix belongs to a
    /// different conversation, silently turning a cache hit into a full
    /// re-prefill (measured on the reference host: ~62ms hit vs ~103ms miss).
    ///
    /// Only meaningful when more than one slot exists: with `-np 1` there is
    /// nothing to pin, so callers pass `None` and keep the old auto behavior
    /// exactly. The hash is stable across processes (FNV-1a, not `DefaultHasher`,
    /// whose seed is randomized per process).
    pub(crate) fn slot_for_session(session_id: &str, parallel_slots: usize) -> Option<i64> {
        if parallel_slots <= 1 || session_id.is_empty() {
            return None;
        }
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for byte in session_id.as_bytes() {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        Some((hash % parallel_slots as u64) as i64)
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

    /// Whether llama-server can actually *serve* a request yet, as opposed to
    /// merely running.
    ///
    /// `/health` answers 200 as soon as the HTTP listener is up, which happens
    /// before the model finishes loading. During that window the inference
    /// endpoints answer `503 {"message":"Loading model"}`. A load path that trusts
    /// `/health` alone therefore reports the model ready and then immediately
    /// issues its warmup generation into a server that is still loading -- which
    /// is how `[model-load] Warmup request failed (ignored)` happens, and why the
    /// first real request after a swap pays a cold-start penalty the warmup was
    /// supposed to remove.
    ///
    /// `/slots` is served only once the slots exist, and while loading it returns
    /// the same 503 as the inference endpoints, so it distinguishes the two states
    /// without generating anything.
    async fn check_llama_server_serving(url: &str) -> bool {
        let base = Self::normalize_llama_base_url(url);
        let client = reqwest::Client::new();
        client
            .get(format!("{base}/slots"))
            .timeout(Duration::from_secs(5))
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
    /// Calculates model-ready timeout in seconds dynamically based on model size (GB),
    /// RPC peer count, and whether distributed inference (--rpc) is active.
    ///
    /// Formula:
    ///   base = 90s (floor)
    ///   size_component = floor(model_size_gb * 15s)
    ///   peer_component = peer_count * 60s (if --rpc present)
    ///   timeout = clamp(base + size_component + peer_component, 90s, 1800s)
    ///
    /// `GHOSTLINK_MODEL_READY_TIMEOUT_SECS` explicitly overrides this calculation.
    pub fn compute_model_ready_timeout(args: &[String], model_size_gb: f32) -> u64 {
        if let Ok(val) = std::env::var("GHOSTLINK_MODEL_READY_TIMEOUT_SECS") {
            if let Ok(n) = val.trim().parse::<u64>() {
                if n > 0 {
                    return n;
                }
            }
        }

        let is_distributed = args.iter().any(|a| a == "--rpc");
        let peer_count = if is_distributed {
            args.iter()
                .position(|a| a == "--rpc")
                .and_then(|idx| args.get(idx + 1))
                .map(|val| val.split(',').filter(|s| !s.trim().is_empty()).count())
                .unwrap_or(1)
        } else {
            0
        };

        let base_secs: u64 = 90;
        let size_add: u64 = (model_size_gb.max(0.0) * 15.0) as u64;
        let peer_add: u64 = (peer_count as u64) * 60;

        let total = base_secs.saturating_add(size_add).saturating_add(peer_add);
        total.clamp(90, 1800)
    }

    fn model_ready_timeout_secs(args: &[String]) -> u64 {
        Self::compute_model_ready_timeout(args, 0.0)
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

        // Two gates: `/health` (process listening) then `/slots` (model actually
        // servable). The second one is what makes the post-load warmup meaningful.
        let mut listening = false;
        while start.elapsed() < timeout {
            if !listening {
                if Self::check_llama_server_health(&base).await {
                    listening = true;
                }
            } else if Self::check_llama_server_serving(&base).await {
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
            "llama-server did not become ready within {} seconds at {} ({})",
            timeout_secs,
            base,
            if listening {
                "/health answered but /slots never did (model still loading?)"
            } else {
                "/health never answered (process not listening?)"
            }
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
    pub fn resolve_model_path(model_path: &str) -> Result<PathBuf, String> {
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
    /// The tuning values actually chosen for a model load, plus any that contradict an
    /// explicit setting.
    ///
    /// Added because the load path was silently discarding user configuration. Measured
    /// on this machine: `settings.json` asked for `ctx_size: 131072` and `ngl: 100`, and
    /// `llama-server` was launched with `-c 4096 -ngl 0`:
    ///
    /// ```text
    /// -m models/Qwen3.8-27B-UD-IQ3_S.gguf -c 4096 -np 1 -ngl 0 -t 15
    /// ```
    ///
    /// Three of four explicit settings were overridden by the model-size branches in
    /// `get_ngl`/`get_ctx_size`/`get_threads`, with nothing logged. A user reading
    /// `settings.json` would have no way to tell. The overrides themselves are

    pub fn load_model_into_slot(
        &self,
        model_path: &str,
        rpc_servers: Option<&str>,
        tensor_split: Option<&str>,
        tensor_override: Option<&str>,
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
        // Remember what this process was actually launched with. The chat path needs
        // it to clamp `conversation_token_limit`: the setting is a user preference and
        // this is derived from VRAM and model size, and they disagree often enough
        // that budgeting against the setting alone produces requests llama-server
        // rejects outright.
        Self::record_running_ctx(ctx);
        // Report the tuning actually applied, and name anything the model-size policy
        // discarded. The existing `[perf-tier]` line covered batch/FA/KV only, so a
        // configured ctx_size or ngl that went unused left no trace at all.
        eprintln!("{}", describe_tuning(model_size_gb));
        let parallel_slots = Self::get_parallel_slots();
        let mlock = Self::get_mlock();
        let no_mmap = Self::get_no_mmap();
        let mut extra_args = Self::get_llama_server_args();
        if let Some(servers) = rpc_servers.filter(|s| !s.is_empty()) {
            // Refuse with the reason, rather than handing llama-server a flag it exits
            // on. Otherwise the failure is a bare "invalid argument: --rpc", which reads
            // like a malformed command line rather than a build configured without
            // distributed-inference support.
            if !Self::binary_supports_rpc() {
                return Err(format!(
                    "distributed inference requested (--rpc {servers}) but the configured \
                     llama-server binary does not support it. This build was made without \
                     GGML_RPC; rebuild llama.cpp with -DGGML_RPC=ON (and build \
                     ggml-rpc-server) to enable cross-machine tensor split, or clear \
                     GHOSTLINK_REQUIRE_CLUSTER_OFFLOAD to stay single-node."
                ));
            }
            eprintln!("[model-load] Distributed inference enabled: --rpc {servers}");
            extra_args.push("--rpc".to_string());
            extra_args.push(servers.to_string());
            if let Some(split) = tensor_split.filter(|s| !s.is_empty()) {
                eprintln!("[model-load] Tensor split: -ts {split}");
                extra_args.push("-ts".to_string());
                extra_args.push(split.to_string());
            }
            // Route by tensor class, not by equal layer share. `-ts` says how much
            // goes where; it does not say *which tensors*, so llama-server spreads whole
            // layers -- attention, KV, lm_head included -- across the link. `-ot` is the
            // flag that names them, and it was documented in a unit test but never
            // passed here, so the intent had no path to the process at all.
            if let Some(ot) = tensor_override.filter(|s| !s.is_empty()) {
                eprintln!("[model-load] Tensor class override: -ot {ot}");
                extra_args.push("-ot".to_string());
                extra_args.push(ot.to_string());
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

            let staging_timeout_secs = Self::compute_model_ready_timeout(args, model_size_gb);
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

        // Memory precondition check, after staging proved the arg set works and
        // immediately before the real spawn.
        //
        // Full GPU offload allocates a *second* copy of the weights in device
        // memory alongside the host copy, so a model that loads comfortably at
        // `-ngl 0` can fail at `-ngl -1` purely because something else grew.
        // Observed on this machine: the 12GB IQ3_S 27B loaded at -ngl -1 twice
        // with ~22GB free, and failed three times with
        // `vk::Device::allocateMemory: ErrorOutOfDeviceMemory` at ~11GB free.
        //
        // The failure mode without this check is nasty: llama-server dies inside
        // Vulkan with a message most operators cannot connect to "something else
        // on the machine is using RAM". So warn while there is still a chance to
        // act -- and warn rather than refuse, because the estimate is conservative
        // and refusing to load a model that would in fact fit would be worse.
        if let Some((available_gb, needed_gb)) = Self::check_offload_memory_headroom(model_size_gb)
        {
            if Self::should_warn_about_memory(available_gb, needed_gb) {
                eprintln!(
                    "[model-load] WARNING: {} GPU-offloaded needs roughly {:.1}GB free (host copy +                      device copy) but only {:.1}GB is available. If this load fails with a Vulkan                      out-of-memory error, close memory-heavy apps or set                      GHOSTLINK_LLAMA_NGL=0 to load CPU-only.",
                    normalized_path, needed_gb, available_gb
                );
            }
        }

        eprintln!("[model-load] Starting llama-server on {port}: {normalized_path}");
        let mut child = build_cmd(port, winning_args).spawn().map_err(|err| {
            format!(
                "failed to start llama-server ('{bin}'): {err}. Ensure the binary exists and port {port} is free."
            )
        })?;

        let pid = child.id();
        eprintln!("[model-load] Started llama-server with PID: {pid}");

        let final_timeout_secs = Self::compute_model_ready_timeout(winning_args, model_size_gb);
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

        // Warm the freshly-loaded server with one throwaway generation. The
        // first real request otherwise pays for graph/allocator/KV-slot setup
        // that nothing else has triggered yet — measured on the reference host
        // as a ~99ms first-token vs ~22-45ms once warm. Deliberately
        // best-effort: a warmup failure must never fail a load that already
        // proved itself healthy above. `load_model_into_slot` is sync, so the
        // warmup is driven on the runtime it already uses for the health wait.
        rt.block_on(Self::warmup_after_load(&base_url));

        Ok(())
    }

    /// Best-effort single-token generation against a just-loaded server.
    /// `max_tokens: 1` keeps the cost to one prefill + one decode; the result
    /// is discarded. Never propagates an error — the load is already confirmed
    /// healthy by the caller.
    async fn warmup_after_load(base_url: &str) {
        let client = reqwest::Client::new();
        let payload = serde_json::json!({
            "model": "warmup",
            "messages": [
                {"role": "system", "content": Self::static_system_prompt()},
                {"role": "user", "content": "warmup"},
            ],
            "max_tokens": 1,
            "temperature": 0.0,
            "stream": false,
            "cache_prompt": true,
        });
        let url = format!("{base_url}/v1/chat/completions");
        match tokio::time::timeout(
            Duration::from_secs(60),
            client
                .post(&url)
                .header("Content-Type", "application/json")
                .json(&payload)
                .send(),
        )
        .await
        {
            Ok(Ok(resp)) if resp.status().is_success() => {
                eprintln!("[model-load] Warmup generation completed");
            }
            Ok(Ok(resp)) => {
                eprintln!(
                    "[model-load] Warmup returned status {} (ignored)",
                    resp.status()
                );
            }
            Ok(Err(err)) => eprintln!("[model-load] Warmup request failed (ignored): {err}"),
            Err(_) => eprintln!("[model-load] Warmup timed out after 60s (ignored)"),
        }
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
    /// Whether the llama-server at the configured URL can actually serve a request.
    ///
    /// `has_running_llama_server` only knows about processes *this* binary launched --
    /// it inspects a stored `Child` handle. That is correct for its purpose (don't leak
    /// a child we own) and useless for deciding whether a request will succeed, because
    /// the common cases are a server Ghostlink did not start and a server that died.
    ///
    /// Measured live, with no llama-server running at all:
    ///
    /// ```text
    /// GET /health   -> {"status":"healthy", ... "uptime_s":41}
    /// POST /api/inference/chat -> 200 after ~20s, containing
    ///     Native error: llama_server request failed: error sending request for url
    ///     (http://127.0.0.1:8080/completion)
    /// ```
    ///
    /// Twenty seconds per request, and the health endpoint reported the whole time.
    /// This asks the thing that actually matters instead: can the server generate?
    ///
    /// Best effort by design -- a probe that cannot reach the server reports `false`,
    /// and a caller that treats that as "load the model" will simply find it already
    /// loaded and continue.
    pub async fn backend_can_generate(&self) -> bool {
        self.backend_can_generate_at(&Self::get_llama_base_url())
            .await
    }

    /// Probe against an explicit base URL. Split out so tests can point at a port that
    /// is guaranteed closed without mutating process-global environment variables,
    /// which would race every other env-reading test in this module.
    pub async fn backend_can_generate_at(&self, base: &str) -> bool {
        let url = format!("{}/completion", base.trim_end_matches('/'));
        // The smallest generation that still exercises slot allocation and the model.
        let body = serde_json::json!({
            "prompt": "hi",
            "n_predict": 1,
            "stream": false,
        })
        .to_string();
        // 2s, not the 10s default. Measured: a health probe against a backend that is
        // not listening took ~2.0s per call, which is slow enough to be a real cost on a
        // health endpoint that a GUI polls. A refused *local* port is instant; the wait
        // is the client's own timeout expiring on a routeless address, so the bound has
        // to be short enough to be harmless.
        let Ok(client) = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(2))
            .build()
        else {
            return false;
        };
        match client
            .post(&url)
            .header("Content-Type", "application/json")
            .body(body)
            .send()
            .await
        {
            Ok(resp) => resp.status().is_success(),
            Err(_) => false,
        }
    }

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
        // Condensed memory of turns trimmed out of the conversation budget.
        // See `generate_with_llama_server`'s doc comment. Ignored by the
        // llama_cpp/simulated paths below.
        history_summary: Option<&str>,
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
                        history_summary,
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
                    // No llama.cpp timings on this path: it is the computed-token
                    // fallback, not a measured generation. Leaving the prompt fields
                    // absent keeps it out of a benchmark as *unmeasured* rather than
                    // reporting a throughput it never observed.
                    prompt_tokens: None,
                    prompt_ms: None,
                    prompt_tokens_per_sec: None,
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
        // Condensed memory of turns that aged out of the conversation budget
        // (see `handle_gui_chat`'s `session_summaries`). Sent as a *second*
        // `system` message right after the static one when present — verified
        // against llama-server's chat template, which honours an extra system
        // turn. `None` for stateless callers and sessions with no trimmed
        // history, which is the prior behavior exactly.
        history_summary: Option<&str>,
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

        // Models have no clock; the current local date/time is appended to the
        // user turn (not the system prompt) so the cached system prefix stays
        // byte-identical across requests — see `static_system_prompt`.
        let system_prompt = Self::static_system_prompt();

        // Try chat completion endpoint first (for models with chat templates)
        let chat_url = format!("{base_url}/v1/chat/completions");

        let mut chat_messages =
            vec![serde_json::json!({"role": "system", "content": system_prompt})];
        if let Some(summary) = history_summary.filter(|s| !s.trim().is_empty()) {
            chat_messages.push(serde_json::json!({
                "role": "system",
                "content": format!("Summary of earlier conversation:\n{}", summary.trim())
            }));
        }
        for (role, content) in history {
            chat_messages.push(serde_json::json!({"role": role, "content": content}));
        }
        chat_messages.push(serde_json::json!({"role": "user", "content": Self::user_turn_with_context(cleaned_prompt)}));

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

        if let Some(ref response_format) = response_format {
            if let Some(obj) = chat_payload.as_object_mut() {
                obj.insert("response_format".to_string(), response_format.clone());
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
        if let Some(summary) = history_summary.filter(|s| !s.trim().is_empty()) {
            // No structured `messages` on this no-chat-template fallback, so
            // the condensed memory is just a labelled block before the turns.
            completion_prompt.push_str(&format!(
                "\n\nSummary of earlier conversation: {}",
                summary.trim()
            ));
        }
        for (role, content) in history {
            let label = if role.eq_ignore_ascii_case("assistant") {
                "Assistant"
            } else {
                "User"
            };
            completion_prompt.push_str(&format!("\n\n{label}: {content}"));
        }
        completion_prompt.push_str(&format!(
            "\n\nUser: {}\n\nAssistant:",
            Self::user_turn_with_context(cleaned_prompt)
        ));

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
        history: &[(String, String)],
        // See `generate_with_llama_server`'s `history_summary`.
        history_summary: Option<&str>,
        id_slot: Option<i64>,
        cache_prompt: bool,
        response_format: Option<serde_json::Value>,
    ) -> Result<NativeChatStream, String> {
        use futures::StreamExt;

        let base_url = Self::get_llama_base_url();
        // Time to wait for llama-server to accept the request and start
        // responding (covers prompt prefill on a cold/uncached slot), not
        // the total generation time.
        let connect_timeout_secs = if std::net::TcpStream::connect("127.0.0.1:8080").is_err() {
            1
        } else {
            std::env::var("GHOSTLINK_LLAMA_CONNECT_TIMEOUT_SECS")
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(30)
                .clamp(5, 120)
        };
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
        let system_prompt = Self::static_system_prompt();
        let chat_url = format!("{base_url}/v1/chat/completions");

        let mut chat_messages =
            vec![serde_json::json!({"role": "system", "content": system_prompt})];
        if let Some(summary) = history_summary.filter(|s| !s.trim().is_empty()) {
            chat_messages.push(serde_json::json!({
                "role": "system",
                "content": format!("Summary of earlier conversation:\n{}", summary.trim())
            }));
        }
        for (role, content) in history {
            chat_messages.push(serde_json::json!({"role": role, "content": content}));
        }
        chat_messages.push(serde_json::json!({"role": "user", "content": Self::user_turn_with_context(cleaned_prompt)}));

        let mut chat_payload = serde_json::json!({
            "model": model,
            "messages": chat_messages,
            "max_tokens": max_tokens,
            "temperature": temperature.clamp(0.0, 2.0),
            "top_p": top_p.clamp(0.0, 1.0),
            "top_k": top_k.clamp(1, 200),
            "repeat_penalty": repeat_penalty.clamp(0.0, 2.0),
            "stream": true,
            "id_slot": id_slot.unwrap_or(-1),
            "cache_prompt": cache_prompt
        });

        if let Some(ref response_format) = response_format {
            if let Some(obj) = chat_payload.as_object_mut() {
                obj.insert("response_format".to_string(), response_format.clone());
                obj.insert(
                    "chat_template_kwargs".to_string(),
                    serde_json::json!({ "enable_thinking": false }),
                );
            }
        }

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
                    history,
                    history_summary,
                    id_slot,
                    cache_prompt,
                    response_format,
                )
                .await?;
            let single = futures::stream::once(async move { Ok(NativeChatEvent::Delta(gen.text)) });
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

        let (tx, rx) = mpsc::channel::<Result<NativeChatEvent, String>>(100);
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
                    let data: LlamaStreamChunk = match serde_json::from_str(payload) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };
                    if let Some(choice) = data.choices.first() {
                        if let Some(delta) = &choice.delta {
                            if let Some(text) = &delta.content {
                                if !text.is_empty()
                                    && tx
                                        .send(Ok(NativeChatEvent::Delta(text.clone())))
                                        .await
                                        .is_err()
                                {
                                    return; // receiver dropped, stop reading
                                }
                            }
                        }
                        if let Some(reason) = choice.finish_reason.as_ref().filter(|r| !r.is_null())
                        {
                            // "length" means the model was cut off at
                            // max_tokens, not that it chose to stop — report
                            // truncation the moment the backend says so,
                            // rather than only on the terminal done-chunk.
                            if reason.as_str() == Some("length")
                                && tx.send(Ok(NativeChatEvent::Truncated)).await.is_err()
                            {
                                return;
                            }
                            return;
                        }
                    }
                }
            }
        });

        Ok(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }
}

#[derive(Debug, serde::Deserialize)]
struct LlamaStreamChunk {
    #[serde(default)]
    choices: Vec<LlamaStreamChoice>,
}

#[derive(Debug, serde::Deserialize)]
struct LlamaStreamChoice {
    #[serde(default)]
    delta: Option<LlamaStreamDelta>,
    #[serde(default)]
    finish_reason: Option<serde_json::Value>,
}

#[derive(Debug, serde::Deserialize)]
struct LlamaStreamDelta {
    #[serde(default)]
    content: Option<String>,
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
    let prompt = parse_prompt_timings(parsed);
    Some(NativeGeneration {
        text,
        real_inference: true,
        tokens_generated,
        tokens_per_sec,
        latency_ms,
        prompt_tokens: prompt.prompt_tokens,
        prompt_ms: prompt.prompt_ms,
        prompt_tokens_per_sec: prompt.prompt_tokens_per_sec,
    })
}

/// A measured prefill/decode split.
///
/// The two are kept apart deliberately. Prefill batches and is compute-bound;
/// decode is memory-bandwidth-bound and does not batch. A single tok/s figure over
/// a whole request averages two unrelated costs and moves for reasons that have
/// nothing to do with each other.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PromptTimings {
    pub prompt_tokens: Option<u32>,
    pub prompt_ms: Option<f32>,
    pub prompt_tokens_per_sec: Option<f32>,
}

/// Reads llama.cpp's prompt-side timings from the response `timings` object.
///
/// These were never read before. The server returns `prompt_n` and `prompt_ms`
/// alongside `predicted_n`/`predicted_ms` in the same object, and only the
/// predicted half was parsed -- so prefill throughput, the number that decides
/// whether a long history is affordable, was unmeasurable. Any local-vs-RPC
/// comparison had to fall back on end-to-end latency, which conflates the two.
///
/// No fallback is invented for an absent field. A missing timing means this build
/// did not report it; deriving one from a whitespace count would put a fabricated
/// number into a benchmark, which is worse than an honest gap.
fn parse_prompt_timings(parsed: &serde_json::Value) -> PromptTimings {
    let timings = parsed.get("timings");
    let prompt_n = timings
        .and_then(|t| t.get("prompt_n"))
        .and_then(|v| v.as_u64())
        .map(|n| n as u32);
    let prompt_ms = timings
        .and_then(|t| t.get("prompt_ms"))
        .and_then(|v| v.as_f64())
        .map(|ms| ms as f32);
    // Prefer llama.cpp's own rate; derive only when both inputs are real. A zero or
    // absent denominator yields None, never 0.0 -- "not measured" and "measured as
    // zero" must not look alike in a report.
    let prompt_tokens_per_sec = timings
        .and_then(|t| t.get("prompt_per_second"))
        .and_then(|v| v.as_f64())
        .filter(|v| *v > 0.0)
        .map(|v| v as f32)
        .or_else(|| match (prompt_n, prompt_ms) {
            (Some(n), Some(ms)) if n > 0 && ms > 0.0 => Some(n as f32 / (ms / 1000.0)),
            _ => None,
        });
    PromptTimings {
        prompt_tokens: prompt_n,
        prompt_ms,
        prompt_tokens_per_sec,
    }
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
    use super::{describe_tuning, parse_prompt_timings, NativeChatEvent, NativeEngineClient, PromptTimings};
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        ENV_LOCK.get_or_init(|| Mutex::new(()))
    }

    /// The static system prompt is the cached prefix llama-server reuses for
    /// `cache_prompt`. It must not embed a timestamp (or anything else that
    /// changes between requests), or the prefix cache can never hit.
    #[test]
    fn static_system_prompt_is_stable_and_has_no_timestamp() {
        let first = NativeEngineClient::static_system_prompt();
        // Sleep past a minute boundary so a per-request `chrono::Local::now()`
        // would demonstrably differ if it had not been split out.
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let second = NativeEngineClient::static_system_prompt();
        assert_eq!(
            first, second,
            "system prompt must be byte-identical across calls"
        );
        assert!(
            !first.contains("Current local date and time"),
            "timestamp must not live in the cached system prefix"
        );
        // No digits at all: a date/time interpolation always introduces some.
        assert!(
            !first.chars().any(|c| c.is_ascii_digit()),
            "static system prompt unexpectedly contains digits: {first}"
        );
    }

    /// The dynamic date/time must actually reach the model, just outside the
    /// cached system prefix (appended to the user turn).
    #[test]
    fn user_turn_appends_dynamic_context_outside_system_prompt() {
        let turn = NativeEngineClient::user_turn_with_context("Hello there");
        assert!(turn.starts_with("Hello there\n\n[Context: current local date and time is "));
        assert!(turn.ends_with(']'));
    }

    /// With a single slot there is nothing to pin — must keep llama-server's
    /// own auto (`-1`) behavior exactly as before.
    #[test]
    fn slot_for_session_is_none_with_a_single_slot() {
        assert_eq!(NativeEngineClient::slot_for_session("sess_a", 1), None);
        assert_eq!(NativeEngineClient::slot_for_session("sess_a", 0), None);
        assert_eq!(NativeEngineClient::slot_for_session("", 8), None);
    }

    /// Same conversation must always map to the same slot (that is the whole
    /// point — a stable prefix cache), stay in range, and be a *stable* value
    /// across processes (FNV-1a, not a randomized hasher).
    #[test]
    fn slot_for_session_is_stable_in_range_and_spreads() {
        let slots = 8usize;
        let a1 = NativeEngineClient::slot_for_session("sess_alpha", slots);
        let a2 = NativeEngineClient::slot_for_session("sess_alpha", slots);
        assert_eq!(a1, a2, "same session must map to the same slot");
        assert!(a1.unwrap() >= 0 && (a1.unwrap() as usize) < slots);

        // Pin an exact value so a change to the hash is caught: a per-process
        // randomized hasher would make this flaky instead of deterministic.
        assert_eq!(
            NativeEngineClient::slot_for_session("sess_local_001", 8),
            Some(3)
        );

        // Different sessions should not all collapse onto one slot.
        let distinct: std::collections::HashSet<i64> = (0..32)
            .map(|i| NativeEngineClient::slot_for_session(&format!("sess_{i}"), slots).unwrap())
            .collect();
        assert!(
            distinct.len() > 1,
            "slot mapping collapsed every session onto one slot: {distinct:?}"
        );
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
    fn model_ready_timeout_scales_with_rpc_flag_and_peers() {
        let _guard = env_lock().lock().expect("env lock poisoned");
        std::env::remove_var("GHOSTLINK_MODEL_READY_TIMEOUT_SECS");
        let args = vec![
            "--rpc".to_string(),
            "10.0.0.29:50052".to_string(),
            "-ts".to_string(),
            "3.9990,0.1000".to_string(),
        ];
        // 90s base + 1 peer * 60s = 150s for 0GB model size
        assert_eq!(
            NativeEngineClient::compute_model_ready_timeout(&args, 0.0),
            150
        );
        // 90s base + 10GB * 15s + 1 peer * 60s = 300s for 10GB model size
        assert_eq!(
            NativeEngineClient::compute_model_ready_timeout(&args, 10.0),
            300
        );
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
                    &[],
                    None,
                    None,
                    false,
                    None,
                )
                .await
                .expect("stream should start");

            let mut chunk_count = 0usize;
            let mut chunk_times = Vec::new();
            let start = std::time::Instant::now();
            let mut full_text = String::new();
            while let Some(item) = stream.next().await {
                let event = item.expect("chunk should not error");
                chunk_count += 1;
                chunk_times.push(start.elapsed());
                if let NativeChatEvent::Delta(text) = event {
                    full_text.push_str(&text);
                }
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
                        None,
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
        // The new user turn carries the prompt plus the dynamic date/time
        // context appended outside the cached system prefix.
        let user_turn = messages[3]["content"]
            .as_str()
            .expect("user turn content should be a string");
        assert!(
            user_turn
                .starts_with("and its population?\n\n[Context: current local date and time is "),
            "unexpected user turn: {user_turn}"
        );
        // The cached system prefix must stay free of the per-request timestamp.
        assert_eq!(
            messages[0]["content"],
            NativeEngineClient::static_system_prompt()
        );
        assert!(!messages[0]["content"]
            .as_str()
            .unwrap()
            .contains("current local date and time"));

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
    #[test]
    fn model_ready_timeout_scales_with_model_size_and_peers() {
        let _guard = env_lock().lock().expect("env lock poisoned");
        std::env::remove_var("GHOSTLINK_MODEL_READY_TIMEOUT_SECS");

        let args_single: Vec<String> = vec![];
        // 0GB model -> 90s base
        assert_eq!(
            NativeEngineClient::compute_model_ready_timeout(&args_single, 0.0),
            90
        );
        // 10GB model -> 90s + 150s = 240s
        assert_eq!(
            NativeEngineClient::compute_model_ready_timeout(&args_single, 10.0),
            240
        );

        let args_dist = vec![
            "--rpc".to_string(),
            "10.0.0.1:50052,10.0.0.2:50052".to_string(),
        ];
        // 10GB model + 2 peers -> 90s + 150s + 120s = 360s
        assert_eq!(
            NativeEngineClient::compute_model_ready_timeout(&args_dist, 10.0),
            360
        );
    }

    #[test]
    fn model_ready_timeout_respects_floor_and_cap() {
        let _guard = env_lock().lock().expect("env lock poisoned");
        std::env::remove_var("GHOSTLINK_MODEL_READY_TIMEOUT_SECS");

        let args_single: Vec<String> = vec![];
        // Floor test: negative or 0GB model -> 90s
        assert_eq!(
            NativeEngineClient::compute_model_ready_timeout(&args_single, -5.0),
            90
        );

        // Cap test: 200GB model + 50 peers -> capped at 1800s
        let args_huge = vec!["--rpc".to_string(), "host1:1,host2:2,host3:3".to_string()];
        assert_eq!(
            NativeEngineClient::compute_model_ready_timeout(&args_huge, 200.0),
            1800
        );
    }

    /// The memory precondition must not fire for CPU-only loads -- there is no
    /// duplicate device allocation to make room for.
    #[test]
    fn memory_headroom_check_is_skipped_for_cpu_only() {
        // get_ngl returns 0 for large models when GHOSTLINK_LLAMA_NGL is unset,
        // so drive the branch directly rather than depending on env.
        assert_eq!(
            NativeEngineClient::check_offload_memory_headroom_for_ngl(12.0, 0),
            None
        );
    }

    /// When offloading, the requirement must exceed the model's own size: the
    /// device buffer is a *second* allocation, not a move.
    #[test]
    fn memory_headroom_required_exceeds_model_size() {
        let (needed, model) = NativeEngineClient::check_offload_memory_headroom_for_ngl(12.0, -1)
            .expect("offloaded load should require a headroom figure");
        assert!(
            needed > model,
            "needed must exceed model size to cover the device copy"
        );
        // 1.35x for a 12GB model.
        assert!(
            (needed - 16.2).abs() < 0.05,
            "expected roughly 16.2GB of headroom"
        );
    }

    /// 12GB at -ngl -1 needs ~16.3GB. Anything below that should warn; this is
    /// the arithmetic that was never visible when llama-server died inside Vulkan.
    #[test]
    fn memory_headroom_flags_the_observed_failure_condition() {
        // Comfortable: 22GB free was observed to load fine, so no warning.
        assert!(!NativeEngineClient::should_warn_about_memory(22.0, 16.2));
        // Tight: ~11GB free produced ErrorOutOfDeviceMemory three times, so warn.
        assert!(NativeEngineClient::should_warn_about_memory(11.0, 16.2));
        // Exactly at the line is not a warning (the test is strictly-less-than).
        assert!(!NativeEngineClient::should_warn_about_memory(16.2, 16.2));
    }

    #[test]
    fn prompt_timings_are_read_from_the_timings_object() {
        // Exactly what llama-server returns. Before this, only the `predicted_*`
        // half was parsed and prefill throughput was unmeasurable.
        let body = serde_json::json!({
            "timings": {
                "prompt_n": 1204,
                "prompt_ms": 812.5,
                "prompt_per_second": 1481.9,
                "predicted_n": 96,
                "predicted_ms": 24576.0,
                "predicted_per_second": 3.91
            }
        });
        let t = parse_prompt_timings(&body);
        assert_eq!(t.prompt_tokens, Some(1204));
        assert_eq!(t.prompt_ms, Some(812.5));
        assert!((t.prompt_tokens_per_sec.unwrap() - 1481.9).abs() < 0.01);
    }

    #[test]
    fn prompt_rate_is_derived_when_only_counts_and_time_are_present() {
        let body = serde_json::json!({ "timings": { "prompt_n": 100, "prompt_ms": 200.0 } });
        let t = parse_prompt_timings(&body);
        assert_eq!(t.prompt_tokens_per_sec, Some(500.0));
    }

    #[test]
    fn absent_prompt_timings_stay_absent_rather_than_becoming_zero() {
        // "Not measured" and "measured as zero" must not look alike in a report.
        let body = serde_json::json!({ "timings": { "predicted_n": 10 } });
        let t = parse_prompt_timings(&body);
        assert_eq!(t.prompt_tokens, None);
        assert_eq!(t.prompt_ms, None);
        assert_eq!(
            t.prompt_tokens_per_sec, None,
            "a missing prefill rate must be None, not Some(0.0)"
        );
    }

    #[test]
    fn a_zero_denominator_yields_none_not_infinity_or_zero() {
        let body = serde_json::json!({ "timings": { "prompt_n": 50, "prompt_ms": 0.0 } });
        let t = parse_prompt_timings(&body);
        assert_eq!(t.prompt_tokens, Some(50));
        assert_eq!(t.prompt_tokens_per_sec, None);
    }

    #[test]
    fn a_zero_reported_rate_falls_back_to_the_derived_one() {
        let body = serde_json::json!({
            "timings": { "prompt_n": 40, "prompt_ms": 100.0, "prompt_per_second": 0.0 }
        });
        let t = parse_prompt_timings(&body);
        assert_eq!(t.prompt_tokens_per_sec, Some(400.0));
    }

    #[test]
    fn no_timings_object_at_all_is_handled() {
        let t = parse_prompt_timings(&serde_json::json!({ "content": "hi" }));
        assert_eq!(
            t,
            PromptTimings {
                prompt_tokens: None,
                prompt_ms: None,
                prompt_tokens_per_sec: None
            }
        );
    }

    #[test]
    fn draft_decoding_is_off_when_no_model_is_configured() {
        let _guard = env_lock().lock().expect("env lock poisoned");
        std::env::remove_var("GHOSTLINK_DRAFT_MODEL");
        assert!(
            NativeEngineClient::draft_model_args().is_empty(),
            "speculative decoding must be opt-in"
        );
    }

    #[test]
    fn a_missing_draft_path_disables_decoding_instead_of_failing_the_load() {
        // The primary model is fine; a bad draft path is an optimization that did not
        // happen, not a reason to refuse to serve.
        let _guard = env_lock().lock().expect("env lock poisoned");
        std::env::set_var(
            "GHOSTLINK_DRAFT_MODEL",
            r"C:\definitely\not\here\draft.gguf",
        );
        assert!(NativeEngineClient::draft_model_args().is_empty());
        std::env::remove_var("GHOSTLINK_DRAFT_MODEL");
    }

    #[test]
    fn draft_knobs_use_flag_names_this_build_actually_parses() {
        // `--draft-max` was REMOVED upstream; this build wants `--spec-draft-n-max`.
        // Emitting the documented name would make llama-server exit on an unknown
        // argument, which is a worse outcome than not enabling the feature at all.
        let _guard = env_lock().lock().expect("env lock poisoned");
        let me = std::env::current_exe().expect("test exe path");
        std::env::set_var("GHOSTLINK_DRAFT_MODEL", &me);
        std::env::set_var("GHOSTLINK_DRAFT_MAX", "8");
        std::env::set_var("GHOSTLINK_DRAFT_P_MIN", "0.2");
        let args = NativeEngineClient::draft_model_args();
        assert_eq!(args[0], "--spec-draft-model");
        assert_eq!(args[1], me);
        assert!(
            args.contains(&"--spec-draft-n-max".to_string()),
            "args: {args:?}"
        );
        assert!(
            !args.contains(&"--draft-max".to_string()),
            "removed flag emitted: {args:?}"
        );
        assert!(
            args.contains(&"--spec-draft-p-min".to_string()),
            "args: {args:?}"
        );
        std::env::remove_var("GHOSTLINK_DRAFT_MODEL");
        std::env::remove_var("GHOSTLINK_DRAFT_MAX");
        std::env::remove_var("GHOSTLINK_DRAFT_P_MIN");
    }

    #[test]
    fn a_zero_or_unparseable_draft_max_is_omitted_not_passed() {
        let _guard = env_lock().lock().expect("env lock poisoned");
        let me = std::env::current_exe().expect("test exe path");
        std::env::set_var("GHOSTLINK_DRAFT_MODEL", &me);
        std::env::set_var("GHOSTLINK_DRAFT_MAX", "0");
        std::env::set_var("GHOSTLINK_DRAFT_P_MIN", "not-a-number");
        let args = NativeEngineClient::draft_model_args();
        assert_eq!(
            args.len(),
            2,
            "only the model flag should survive: {args:?}"
        );
        std::env::remove_var("GHOSTLINK_DRAFT_MODEL");
        std::env::remove_var("GHOSTLINK_DRAFT_MAX");
        std::env::remove_var("GHOSTLINK_DRAFT_P_MIN");
    }

    #[test]
    fn hybrid_thread_detection_is_bounded_and_scaled_correctly() {
        // This host is genuinely hybrid, which the tuning doc did not say. Read
        // directly from GetLogicalProcessorInformationEx(RelationProcessorCore):
        //
        //     8 processor-core records, EfficiencyClass = [1,0,1,0,1,0,1,0]
        //     => 4 performance cores, 4 efficiency cores, 8 physical, 16 logical
        //
        // So `performance_cores` is 4, and scaling it by logical/physical
        // (4 * 16 / 8) gives 8 threads -- the P-cores with SMT, which is what `-t`
        // should be. It is not the physical P-core count (4) and not the logical
        // count (16).
        match super::hybrid_performance_threads(16) {
            Some(n) => {
                assert!(n >= 1, "thread pool collapsed to {n}");
                assert!(n <= 16, "thread pool {n} exceeds the logical core count");
                assert!(
                    n >= 8,
                    "expected the P-core pool scaled by SMT (~8), got {n}"
                );
            }
            None => {
                // Acceptable only on a CPU with no real P/E split, where falling back
                // to available_parallelism is correct. Recorded rather than asserted
                // so a failure here names the host instead of a magic number.
                eprintln!(
                    "note: no hybrid split detected; -t would fall back to all logical cores"
                );
            }
        }
    }

    #[test]
    fn hybrid_detection_never_returns_more_threads_than_are_logical() {
        // The failure mode that matters: handing llama.cpp a thread count above the
        // logical core count. Oversubscription on a matmul loop is a large slowdown,
        // not a small one.
        for logical in [1usize, 2, 4, 8, 16, 32, 64] {
            if let Some(n) = super::hybrid_performance_threads(logical) {
                assert!(n >= 1, "logical={logical} produced {n}");
                assert!(n <= logical, "logical={logical} produced {n}");
            }
        }
    }

    #[test]
    fn get_threads_is_never_zero() {
        let _guard = env_lock().lock().expect("env lock poisoned");
        std::env::remove_var("GHOSTLINK_LLAMA_THREADS");
        assert!(NativeEngineClient::get_threads() >= 1);
    }

    #[test]
    fn an_explicit_thread_override_still_wins() {
        let _guard = env_lock().lock().expect("env lock poisoned");
        std::env::set_var("GHOSTLINK_LLAMA_THREADS", "3");
        assert_eq!(NativeEngineClient::get_threads(), 3);
        std::env::remove_var("GHOSTLINK_LLAMA_THREADS");
    }

    #[tokio::test]
    async fn backend_can_generate_is_false_when_nothing_is_listening() {
        // The availability bug this exists for: with no llama-server running, a chat
        // request used to spend ~20s in transport retries and then return HTTP 200
        // containing "Native error: llama_server request failed: error sending request
        // for url (http://127.0.0.1:8080/completion)".
        //
        // Port 1 is reserved and never has a listener, so this cannot pass by accident.
        let client = NativeEngineClient::new();
        assert!(
            !client.backend_can_generate_at("http://127.0.0.1:1").await,
            "a dead backend must not report itself able to generate"
        );
    }

    #[tokio::test]
    async fn backend_can_generate_does_not_hang_on_a_dead_backend() {
        // The probe sits on the request path, so it has to be fast when the answer is
        // "no". A connect to a refused local port is immediate; the assertion is that
        // the whole thing stays well under the 10s client timeout.
        let client = NativeEngineClient::new();
        let started = std::time::Instant::now();
        let _ = client.backend_can_generate_at("http://127.0.0.1:1").await;
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "probe took {:?}; it is on the hot path and must fail fast",
            started.elapsed()
        );
    }

    #[test]
    fn describe_tuning_names_a_setting_the_policy_overrode() {
        // The defect this exists for: settings.json asked for ctx 131072 / ngl 100 and
        // llama-server was launched with -c 4096 -ngl 0, with nothing logged.
        let _guard = env_lock().lock().expect("env lock poisoned");
        for v in [
            "GHOSTLINK_CTX_SIZE",
            "GHOSTLINK_LLAMA_NGL",
            "GHOSTLINK_LLAMA_THREADS",
            "GHOSTLINK_VRAM_GB",
        ] {
            std::env::remove_var(v);
        }
        // Note the env vars are NOT set here, and that is the point.
        //
        // `settings.json` carries `ctx_size: 131072` and `ngl: 100` alongside
        // `ctx_size_auto: true` / `ngl_auto: true`, so `apply_native_engine_tuning_env`
        // never exports them and the getters fall through to the model-size policy --
        // which is how the server was launched with `-c 4096 -ngl 0` and nothing logged.
        //
        // Setting the env vars directly would *win*, because the getters read them first.
        // So the override only ever happens through the auto path, which is exactly the
        // path that was silent.
        let line = describe_tuning(12.0);
        assert!(
            line.contains("-c 4096"),
            "a 12 GB model must cap ctx at 4096: {line}"
        );
        assert!(
            line.contains("-ngl 0"),
            "a 12 GB model must be CPU-only here: {line}"
        );

        // Now the explicit case: with the env var set, the getter honours it and there is
        // nothing to report. Both halves of the contract in one test.
        std::env::set_var("GHOSTLINK_CTX_SIZE", "131072");
        let explicit = describe_tuning(12.0);
        assert!(
            explicit.contains("-c 131072"),
            "an explicit request must be honoured: {explicit}"
        );
        assert!(
            !explicit.contains("OVERRIDDEN"),
            "an honoured request must not be reported as overridden: {explicit}"
        );
        std::env::remove_var("GHOSTLINK_CTX_SIZE");
    }

    #[test]
    fn describe_tuning_is_quiet_when_nothing_is_overridden() {
        let _guard = env_lock().lock().expect("env lock poisoned");
        for v in [
            "GHOSTLINK_CTX_SIZE",
            "GHOSTLINK_LLAMA_NGL",
            "GHOSTLINK_LLAMA_THREADS",
            "GHOSTLINK_VRAM_GB",
        ] {
            std::env::remove_var(v);
        }
        let line = describe_tuning(1.7);
        assert!(
            line.contains("-c "),
            "must always report effective values: {line}"
        );
        assert!(
            !line.contains("OVERRIDDEN"),
            "nothing was overridden, so it must not claim otherwise: {line}"
        );
    }

    #[test]
    fn describe_tuning_reports_the_effective_values() {
        let _guard = env_lock().lock().expect("env lock poisoned");
        for v in [
            "GHOSTLINK_CTX_SIZE",
            "GHOSTLINK_LLAMA_NGL",
            "GHOSTLINK_LLAMA_THREADS",
            "GHOSTLINK_VRAM_GB",
        ] {
            std::env::remove_var(v);
        }
        let line = crate::native_engine::describe_tuning(1.7);
        assert!(
            line.contains("-c "),
            "must always report effective values: {line}"
        );
        assert!(line.contains("-ngl "), "must report ngl: {line}");
        assert!(line.contains("-t "), "must report threads: {line}");
    }

    #[test]
    fn describe_tuning_reports_the_policy_choice_when_nothing_was_requested() {
        // With the env unset, the model-size policy decides. On this hardware a 12 GB
        // model is capped at 4096 ctx and forced CPU-only, and that is what must be shown.
        let _guard = env_lock().lock().expect("env lock poisoned");
        for v in [
            "GHOSTLINK_CTX_SIZE",
            "GHOSTLINK_LLAMA_NGL",
            "GHOSTLINK_LLAMA_THREADS",
            "GHOSTLINK_VRAM_GB",
        ] {
            std::env::remove_var(v);
        }
        let line = crate::native_engine::describe_tuning(12.0);
        assert!(
            line.contains("-c 4096"),
            "a 12 GB model caps ctx at 4096: {line}"
        );
        assert!(
            line.contains("-ngl 0"),
            "a 12 GB model is CPU-only here: {line}"
        );
    }

    #[test]
    fn an_explicit_env_value_is_honoured_and_not_called_an_override() {
        // The getters read the env first, so a requested value wins and there is nothing
        // to report. This is the case that must stay quiet -- honouring a request is not
        // an override.
        let _guard = env_lock().lock().expect("env lock poisoned");
        for v in [
            "GHOSTLINK_CTX_SIZE",
            "GHOSTLINK_LLAMA_NGL",
            "GHOSTLINK_LLAMA_THREADS",
            "GHOSTLINK_VRAM_GB",
        ] {
            std::env::remove_var(v);
        }
        std::env::set_var("GHOSTLINK_LLAMA_NGL", "24");
        let line = crate::native_engine::describe_tuning(12.0);
        assert!(
            line.contains("-ngl 24"),
            "an explicit request must be honoured even for a large model: {line}"
        );
        assert!(
            !line.contains("OVERRIDDEN"),
            "an honoured request must not be reported as an override: {line}"
        );
        std::env::remove_var("GHOSTLINK_LLAMA_NGL");
    }

    #[test]
    fn describe_tuning_does_not_report_an_override_when_the_env_wins() {
        // The explicit case is the one that must stay quiet: honouring a request is not
        // an override.
        let _guard = env_lock().lock().expect("env lock poisoned");
        for v in [
            "GHOSTLINK_CTX_SIZE",
            "GHOSTLINK_LLAMA_NGL",
            "GHOSTLINK_LLAMA_THREADS",
            "GHOSTLINK_VRAM_GB",
        ] {
            std::env::remove_var(v);
        }
        std::env::set_var("GHOSTLINK_CTX_SIZE", "131072");
        let line = crate::native_engine::describe_tuning(12.0);
        assert!(
            line.contains("-c 131072"),
            "an explicit request is honoured: {line}"
        );
        assert!(
            !line.contains("OVERRIDDEN"),
            "an honoured request must not be called an override: {line}"
        );
        std::env::remove_var("GHOSTLINK_CTX_SIZE");
    }

    #[test]
    fn describe_tuning_agrees_with_the_functions_it_describes() {
        // The report must not drift from reality: it calls the same getters.
        let _guard = env_lock().lock().expect("env lock poisoned");
        for v in [
            "GHOSTLINK_CTX_SIZE",
            "GHOSTLINK_LLAMA_NGL",
            "GHOSTLINK_LLAMA_THREADS",
            "GHOSTLINK_VRAM_GB",
        ] {
            std::env::remove_var(v);
        }
        for size in [0.7f32, 1.7, 4.85, 12.0, 14.7] {
            let line = crate::native_engine::describe_tuning(size);
            assert!(
                line.contains(&format!("-c {}", NativeEngineClient::get_ctx_size(size))),
                "ctx mismatch at {size} GB: {line}"
            );
            assert!(
                line.contains(&format!("-ngl {}", NativeEngineClient::get_ngl(size))),
                "ngl mismatch at {size} GB: {line}"
            );
        }
    }

    #[test]
    fn auto_ignored_setting_names_a_non_default_value_left_dormant_by_auto() {
        // The more misleading failure mode: settings.json shows a number that is never
        // used, because `*_auto` is true so nothing exports it.
        let dir = std::env::temp_dir().join("ghostlink-tuning-diag-test");
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("settings.json");
        std::fs::write(
            &path,
            r#"{"ctx_size":131072,"ctx_size_auto":true,"ngl":100,"ngl_auto":true,
                "threads":4,"threads_auto":true}"#,
        )
        .expect("write");

        assert_eq!(
            crate::native_engine::auto_ignored_setting_in("ctx_size", &path).as_deref(),
            Some("ctx_size=131072")
        );
        assert_eq!(
            crate::native_engine::auto_ignored_setting_in("ngl", &path).as_deref(),
            Some("ngl=100")
        );
        // threads=4 is the default, so it must NOT be reported: listing defaults would
        // bury the real finding in noise.
        assert_eq!(
            crate::native_engine::auto_ignored_setting_in("threads", &path),
            None,
            "a default value is not an ignored override"
        );
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn auto_ignored_setting_is_silent_when_auto_is_false() {
        // With auto=false the value IS applied, so there is nothing to report.
        let dir = std::env::temp_dir().join("ghostlink-tuning-diag-explicit");
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("settings.json");
        std::fs::write(&path, r#"{"ctx_size":131072,"ctx_size_auto":false}"#).expect("write");
        assert_eq!(
            crate::native_engine::auto_ignored_setting_in("ctx_size", &path),
            None
        );
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn auto_ignored_setting_tolerates_a_missing_or_malformed_file() {
        let missing = std::env::temp_dir().join("ghostlink-tuning-diag-absent.json");
        std::fs::remove_file(&missing).ok();
        assert_eq!(
            crate::native_engine::auto_ignored_setting_in("ngl", &missing),
            None
        );
        let bad = std::env::temp_dir().join("ghostlink-tuning-diag-bad.json");
        std::fs::write(&bad, "{not json").expect("write");
        assert_eq!(
            crate::native_engine::auto_ignored_setting_in("ngl", &bad),
            None
        );
        std::fs::remove_file(&bad).ok();
    }
}
