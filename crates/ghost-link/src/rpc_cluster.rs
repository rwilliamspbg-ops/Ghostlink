//! Orchestration for real cross-machine model-parallel inference via
//! llama.cpp's own RPC backend (`ggml-rpc`).
//!
//! `ghostlink_core::runtime`'s pipeline execution (`ghost-link flow`,
//! `stage-worker`) moves pipeline-shaped *synthetic benchmark* payloads
//! between processes to prove out ring-buffer/TCP-bridge latency — it does
//! not run real model layers. Real distributed inference goes through
//! llama.cpp's RPC backend instead: a node that opts in to contributing
//! compute runs `ggml-rpc-server`, exposing its GPU/CPU as a device over
//! TCP; the node serving a chat request launches its local `llama-server`
//! with `--rpc host:port,...` plus a computed `--tensor-split`, and
//! llama.cpp's own backend scheduler does the real cross-process tensor
//! execution — verified live (see the PR this shipped in) with a model
//! forced entirely (`-ts 0,1`) onto a second process's device.
//!
//! SECURITY: `ggml-rpc-server` has no built-in authentication — this is
//! upstream llama.cpp behavior, not a Ghostlink limitation. Anyone who can
//! reach the port can submit compute to it. `contribute_compute` is off by
//! default for exactly this reason.
//!
//! When it is on, `rpc_allowed_peers` (settings.json) closes the biggest gap
//! in that: an IP allowlist (plain IPv4 addresses or IPv4 CIDR ranges)
//! enforced by Ghostlink itself, since the vendored `ggml-rpc-server` binary
//! has no concept of authentication to patch in. The mechanism is a TCP
//! proxy (`run_rpc_allowlist_proxy`): `ggml-rpc-server` binds loopback-only
//! (unreachable from the network directly) and this process listens on the
//! publicly-advertised `bind_host:port` instead, splicing through only
//! connections whose source IP passes the allowlist and closing everything
//! else immediately. Left empty (the default), no proxy is started at all —
//! `ggml-rpc-server` binds the public address directly, identical to the
//! behavior before this allowlist existed.
//!
//! Be clear-eyed about what this does and doesn't buy: it is real,
//! meaningful access control — "only these hosts/subnets," not "anyone on
//! the LAN." It is **not** authentication of the RPC protocol itself. A
//! device that is itself inside an allowlisted range, or one that can spoof
//! a source IP on the local network, is not stopped by this. Only enable
//! `contribute_compute` (allowlisted or not) on a network you trust, same
//! assumption the existing UDP/mDNS discovery already makes.
//!
//! `rpc_shared_secret` (settings.json) closes that specific gap: a real
//! pre-connection handshake, not just a source-address check. When
//! configured, a contributing node also listens on a small dedicated **auth
//! port** (`rpc_port + RPC_AUTH_PORT_OFFSET`). Before a coordinator opens
//! the real `--rpc` connection, it first connects there, receives a fresh
//! random nonce, and sends back `HMAC-SHA256(rpc_shared_secret, nonce)`. A
//! match temporarily admits that source IP (`admit_peer`, `RPC_ADMISSION_TTL`)
//! — the allowlist proxy then requires a *live* admission in addition to
//! the static IP allowlist for that source before splicing the real
//! connection through. A fresh nonce per handshake means a captured
//! response can't be replayed against a later connection.
//!
//! `llama-server`'s own `--rpc` connection code (upstream llama.cpp) speaks
//! zero custom handshake — it starts sending raw `ggml-rpc` binary protocol
//! the instant it connects. So this handshake cannot live inside that same
//! TCP stream; it's a separate connection, done by Ghostlink's own processes
//! on both ends, purely to admit a source IP before `llama-server` ever
//! dials the real port.
//!
//! Scope, honestly stated: this proves the connecting node held the secret
//! *at admission time* and temporarily authorizes its current source IP —
//! it does **not** encrypt or authenticate the actual `ggml-rpc` byte stream
//! itself (still plain TCP, the same upstream limitation as above), and an
//! on-path attacker able to inject traffic from the same source IP during
//! the admission window isn't stopped by it. True wire-level confidentiality
//! would need a dual-proxy TLS tunnel on both ends (reusing `tls.rs`'s
//! existing cert infra) — a larger follow-up, not attempted here. Empty (the
//! default), no auth port is started and the allowlist proxy's behavior is
//! completely unchanged from before this existed.

use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime};

use tokio::io::copy_bidirectional;
use tokio::net::{TcpListener, TcpStream};
use tokio::runtime::Handle;

use ghostlink_core::cluster::{ClusterState, NodeStatus};

type HmacSha256 = Hmac<Sha256>;

/// Offset added to the publicly-advertised `rpc_port` to derive the port
/// the RPC-auth handshake listener binds to, when `rpc_shared_secret` is
/// configured. Distinct from `RPC_INTERNAL_PORT_OFFSET` (a different
/// mechanism, the loopback `ggml-rpc-server` bind).
const RPC_AUTH_PORT_OFFSET: u16 = 2000;

/// How long a successful handshake admits a source IP for. Long enough to
/// cover the real `--rpc` connection immediately following it, short enough
/// to keep the exposure window narrow.
const RPC_ADMISSION_TTL: Duration = Duration::from_secs(30);

/// Server-side per-step timeout for the auth handshake (write nonce, read
/// MAC) — bounds how long a slow or hung connection can occupy a task.
const RPC_AUTH_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);

/// Client-side timeout for the whole `admit_via_secret` attempt (connect +
/// full handshake) — kept short so one unreachable/misconfigured peer can't
/// meaningfully delay a distributed model load.
const RPC_AUTH_CLIENT_TIMEOUT: Duration = Duration::from_secs(3);

const RPC_NONCE_LEN: usize = 16;
const RPC_MAC_LEN: usize = 32; // HMAC-SHA256 output size

/// Computes `HMAC-SHA256(secret, nonce)` — shared by the server-side
/// handshake handler (verifying a client's response) and `admit_via_secret`
/// (producing one). HMAC accepts a key of any length per RFC 2104 (long
/// keys are hashed down internally), so this never fails in practice.
fn compute_hmac(secret: &str, nonce: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .expect("HMAC-SHA256 accepts a key of any length, per RFC 2104");
    mac.update(nonce);
    mac.finalize().into_bytes().to_vec()
}

static RPC_ADMISSIONS: OnceLock<Arc<Mutex<HashMap<IpAddr, Instant>>>> = OnceLock::new();

fn admissions_registry() -> Arc<Mutex<HashMap<IpAddr, Instant>>> {
    RPC_ADMISSIONS
        .get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
        .clone()
}

/// Pure expiry check, split out from the process-global registry so it's
/// unit-testable against constructed `Instant` values without waiting on
/// real wall-clock time — matching this file's existing
/// `run_rpc_allowlist_proxy`/`serve_rpc_allowlist_proxy` split.
fn admission_is_valid(expiry: Instant, now: Instant) -> bool {
    now < expiry
}

/// Records `ip` as admitted for `RPC_ADMISSION_TTL` from now. Called by the
/// auth-port handshake handler on a successful HMAC match.
fn admit_peer(ip: IpAddr) {
    let registry = admissions_registry();
    let mut guard = registry.lock().unwrap_or_else(|p| p.into_inner());
    guard.insert(ip, Instant::now() + RPC_ADMISSION_TTL);
}

/// Whether `ip` currently holds a live admission grant. Opportunistically
/// prunes expired entries on every check — the registry only ever holds as
/// many entries as recently-handshaking peers, so this stays cheap.
fn is_admitted(ip: &IpAddr) -> bool {
    let registry = admissions_registry();
    let mut guard = registry.lock().unwrap_or_else(|p| p.into_inner());
    let now = Instant::now();
    guard.retain(|_, expiry| admission_is_valid(*expiry, now));
    guard.contains_key(ip)
}

/// Pure gating decision for one inbound proxy connection, split out from
/// `serve_rpc_allowlist_proxy` so it's unit-testable directly — without
/// real sockets or the process-global admissions registry, which every
/// loopback-socket test in this file's suite shares a single source IP
/// against (127.0.0.1), making "is NOT admitted" unreliable to assert from
/// a real connection once any other test has legitimately admitted it.
fn connection_allowed(ip_ok: bool, require_admission: bool, admitted: bool) -> bool {
    ip_ok && (!require_admission || admitted)
}

/// Offset added to the publicly-advertised `rpc_port` to derive the
/// loopback-only port `ggml-rpc-server` actually binds to when the
/// allowlist proxy is active. Arbitrary but fixed, so the proxy and
/// `ggml-rpc-server` agree on it across the periodic respawn-supervision
/// calls without needing to thread extra state between them.
/// `saturating_add`ed against the configured port, so a `rpc_port` already
/// close to `u16::MAX` degrades (a possible port reuse) rather than
/// overflowing/panicking.
const RPC_INTERNAL_PORT_OFFSET: u16 = 1000;

/// Minimum `NodeMetrics::delivery_ratio` a peer must have to be offered as
/// an `--rpc` target in `discover_rpc_peers`. Matches
/// `planning::RebalanceTrigger::default()`'s `0.90` delivery-ratio cutoff
/// for consistency — unlike that trigger's latency threshold, delivery
/// ratio is a plain 0.0-1.0 fraction with no cross-module unit ambiguity,
/// so it's safe to reuse the same number here.
const RPC_PEER_MIN_DELIVERY_RATIO: f32 = 0.90;

/// True once this process has spawned its (single, process-lifetime)
/// allowlist proxy task. Guards `maybe_start_allowlist_proxy` so the
/// periodic respawn-supervision loop in `main.rs` (which calls
/// `ensure_contributing` every 30s) doesn't try to bind the public port a
/// second time.
static RPC_ALLOWLIST_PROXY_STARTED: OnceLock<()> = OnceLock::new();

const MAX_CONSECUTIVE_RESTARTS: u32 = 10;
const INITIAL_BACKOFF_SECS: u64 = 1;
const MAX_BACKOFF_SECS: u64 = 30;

#[derive(Debug)]
pub struct RpcSupervisor {
    child: Option<Child>,
    pid: Option<u32>,
    start_time: Option<SystemTime>,
    last_exit_status: Option<String>,
    restart_count: u32,
    consecutive_failures: u32,
    last_spawn_attempt: Option<Instant>,
    last_health_check: Option<Instant>,
    last_health_result: bool,
    spawn_host: String,
    spawn_port: u16,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct RpcSupervisorInfo {
    pub pid: Option<u32>,
    pub start_time: Option<String>,
    pub last_exit_status: Option<String>,
    pub restart_count: u32,
    pub consecutive_failures: u32,
    pub is_healthy: bool,
}

impl Default for RpcSupervisor {
    fn default() -> Self {
        Self {
            child: None,
            pid: None,
            start_time: None,
            last_exit_status: None,
            restart_count: 0,
            consecutive_failures: 0,
            last_spawn_attempt: None,
            last_health_check: None,
            last_health_result: false,
            spawn_host: "127.0.0.1".to_string(),
            spawn_port: 50052,
        }
    }
}

impl RpcSupervisor {
    pub fn check_child_status(&mut self) -> bool {
        if let Some(child) = self.child.as_mut() {
            match child.try_wait() {
                Ok(Some(status)) => {
                    let exit_str = status.to_string();
                    tracing::warn!(
                        "rpc_cluster: ggml-rpc-server child (PID {:?}) exited with status: {}",
                        self.pid,
                        exit_str
                    );
                    self.last_exit_status = Some(exit_str);
                    self.child = None;
                    self.pid = None;
                    self.last_health_result = false;
                    self.last_health_check = Some(Instant::now());
                    false
                }
                Ok(None) => {
                    if let Some(last_check) = self.last_health_check {
                        if last_check.elapsed() < Duration::from_millis(500) {
                            return self.last_health_result;
                        }
                    }

                    let addr_str = format!("{}:{}", self.spawn_host, self.spawn_port);
                    let healthy =
                        if let Ok(addrs) = std::net::ToSocketAddrs::to_socket_addrs(&addr_str) {
                            let mut ok = false;
                            for addr in addrs {
                                if std::net::TcpStream::connect_timeout(
                                    &addr,
                                    Duration::from_millis(200),
                                )
                                .is_ok()
                                {
                                    ok = true;
                                    break;
                                }
                            }
                            ok
                        } else {
                            false
                        };

                    if healthy {
                        self.consecutive_failures = 0;
                    } else {
                        tracing::warn!(
                            "rpc_cluster: ggml-rpc-server process (PID {:?}) running but not listening on {}:{}",
                            self.pid,
                            self.spawn_host,
                            self.spawn_port
                        );
                    }
                    self.last_health_check = Some(Instant::now());
                    self.last_health_result = healthy;
                    healthy
                }
                Err(err) => {
                    tracing::warn!(
                        "rpc_cluster: error querying ggml-rpc-server child (PID {:?}): {}",
                        self.pid,
                        err
                    );
                    self.last_health_result = false;
                    self.last_health_check = Some(Instant::now());
                    false
                }
            }
        } else {
            self.last_health_result = false;
            false
        }
    }

    pub fn is_healthy(&mut self) -> bool {
        self.check_child_status()
    }

    pub fn get_info(&mut self) -> RpcSupervisorInfo {
        let healthy = self.check_child_status();
        let start_time_str = self.start_time.map(|st| {
            st.duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| format!("{}", d.as_secs()))
                .unwrap_or_default()
        });
        RpcSupervisorInfo {
            pid: self.pid,
            start_time: start_time_str,
            last_exit_status: self.last_exit_status.clone(),
            restart_count: self.restart_count,
            consecutive_failures: self.consecutive_failures,
            is_healthy: healthy,
        }
    }

    #[allow(dead_code)]
    pub fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.pid = None;
        self.last_health_result = false;
        self.last_health_check = None;
    }
}

static RPC_SUPERVISOR: OnceLock<Arc<Mutex<RpcSupervisor>>> = OnceLock::new();

fn supervisor_handle() -> Arc<Mutex<RpcSupervisor>> {
    RPC_SUPERVISOR
        .get_or_init(|| Arc::new(Mutex::new(RpcSupervisor::default())))
        .clone()
}

pub fn is_contributing_healthy() -> bool {
    let handle = supervisor_handle();
    let mut sup = match handle.lock() {
        Ok(guard) => guard,
        Err(p) => p.into_inner(),
    };
    sup.is_healthy()
}

pub fn get_rpc_supervisor_info() -> RpcSupervisorInfo {
    let handle = supervisor_handle();
    let mut sup = match handle.lock() {
        Ok(guard) => guard,
        Err(p) => p.into_inner(),
    };
    sup.get_info()
}

/// Resolves the `ggml-rpc-server` binary, mirroring
/// `native_engine::NativeEngineClient::get_llama_server_bin`'s discovery
/// order: explicit env var, then the same `third_party` build tree.
pub fn get_rpc_server_bin() -> String {
    if let Ok(bin) = std::env::var("GHOSTLINK_RPC_SERVER_BIN") {
        let bin = bin.trim().to_string();
        if !bin.is_empty() && Path::new(&bin).exists() {
            return bin;
        }
    }

    let relative_candidates = [
        "third_party/llama.cpp/build/bin/ggml-rpc-server",
        "third_party/llama.cpp/build/bin/Release/ggml-rpc-server.exe",
        "third_party/llama.cpp/build/bin/ggml-rpc-server.exe",
        "bin/ggml-rpc-server",
        "bin/ggml-rpc-server.exe",
    ];

    let mut search_roots = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        search_roots.push(cwd);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            search_roots.push(parent.to_path_buf());
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
    }

    if cfg!(windows) {
        "ggml-rpc-server.exe".to_string()
    } else {
        "ggml-rpc-server".to_string()
    }
}

/// Starts this node's `ggml-rpc-server` if it isn't already running,
/// exposing local compute to the cluster so other nodes' `--rpc` can reach
/// it. Idempotent — safe to call on every settings load/update. Best-effort:
/// a failure to start is logged, not fatal, mirroring how UDP/mDNS discovery
/// failures never block server startup elsewhere in this codebase.
///
/// When `allowed_peers` and/or `shared_secret` is configured,
/// `ggml-rpc-server` is bound loopback-only and an allowlist proxy (spawned
/// onto `rt_handle`, since this function itself isn't async and may be
/// called before the tokio runtime's `block_on` starts) is stood up on the
/// publicly-advertised `bind_host:port` in front of it — see this module's
/// top-of-file SECURITY doc. A non-empty `shared_secret` also starts the
/// RPC-auth handshake listener and makes the proxy require a live admission
/// grant, not just allowlist membership. When both are empty,
/// `ggml-rpc-server` binds `bind_host:port` directly, exactly as before
/// either existed: no proxy, no extra hop, no behavior change for the
/// default case.
pub fn ensure_contributing(
    bind_host: &str,
    port: u16,
    allowed_peers: &[String],
    shared_secret: &str,
    rt_handle: &Handle,
) {
    let handle = supervisor_handle();
    let mut sup = match handle.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };

    let use_proxy = !allowed_peers.is_empty() || !shared_secret.is_empty();
    let (spawn_host, spawn_port): (String, u16) = if !use_proxy {
        (bind_host.to_string(), port)
    } else {
        let internal_port = port.saturating_add(RPC_INTERNAL_PORT_OFFSET);
        maybe_start_allowlist_proxy(
            bind_host,
            port,
            internal_port,
            allowed_peers,
            shared_secret,
            rt_handle,
        );
        ("127.0.0.1".to_string(), internal_port)
    };
    maybe_start_auth_port(bind_host, port, shared_secret, rt_handle);

    sup.spawn_host = spawn_host.clone();
    sup.spawn_port = spawn_port;

    if sup.check_child_status() {
        return; // already running and listening
    }

    if sup.consecutive_failures >= MAX_CONSECUTIVE_RESTARTS {
        tracing::error!(
            "rpc_cluster: ggml-rpc-server reached max consecutive restarts ({}); supervision paused.",
            MAX_CONSECUTIVE_RESTARTS
        );
        return;
    }

    if let Some(last_attempt) = sup.last_spawn_attempt {
        let exp_secs = INITIAL_BACKOFF_SECS.saturating_mul(1 << sup.consecutive_failures.min(5));
        let backoff = Duration::from_secs(exp_secs.min(MAX_BACKOFF_SECS));
        if last_attempt.elapsed() < backoff {
            tracing::info!(
                "rpc_cluster: backing off ggml-rpc-server restart ({:?} remaining)",
                backoff.saturating_sub(last_attempt.elapsed())
            );
            return;
        }
    }

    sup.last_spawn_attempt = Some(Instant::now());
    let bin = get_rpc_server_bin();
    if !use_proxy {
        tracing::warn!(
            "rpc_cluster: starting ggml-rpc-server on {spawn_host}:{spawn_port} \u{2014} this \
             exposes local compute (GPU/CPU) to the network with NO AUTHENTICATION (an upstream \
             llama.cpp limitation, not Ghostlink's), NO IP ALLOWLIST (rpc_allowed_peers is \
             empty), and NO SHARED-SECRET HANDSHAKE (rpc_shared_secret is empty). Only enable \
             contribute_compute on a network you trust, the same assumption UDP/mDNS discovery \
             already makes. Set rpc_allowed_peers and/or rpc_shared_secret in settings to \
             restrict which hosts may submit compute jobs."
        );
    } else {
        tracing::warn!(
            "rpc_cluster: starting ggml-rpc-server on loopback ({spawn_host}:{spawn_port}), \
             fronted by an allowlist proxy on {bind_host}:{port} restricted to {} \
             rpc_allowed_peers entries{} \u{2014} ggml-rpc-server itself still has NO \
             AUTHENTICATION of its own (an upstream llama.cpp limitation).",
            allowed_peers.len(),
            if shared_secret.is_empty() {
                ", with NO rpc_shared_secret handshake required \u{2014} a device already \
                 inside an allowlisted range isn't stopped by IP allowlisting alone"
                    .to_string()
            } else {
                ", and requiring a live rpc_shared_secret handshake admission before splicing \
                 any connection through"
                    .to_string()
            }
        );
    }

    // ggml-rpc-server's own stdout/stderr (connection/tensor-transfer activity
    // logged by upstream llama.cpp) is discarded by default — matches every
    // other best-effort child process in this codebase. Opt-in redirection to
    // a file via GHOSTLINK_RPC_SERVER_LOG exists specifically so its activity
    // can be inspected/proven from outside the process (e.g. in a container
    // where the contributor's own log is the only evidence a peer actually
    // reached it), without changing default behavior for anyone who hasn't
    // set it.
    let log_file = std::env::var("GHOSTLINK_RPC_SERVER_LOG")
        .ok()
        .filter(|p| !p.trim().is_empty())
        .and_then(|path| {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .map_err(|err| {
                    tracing::warn!(
                        "rpc_cluster: failed to open GHOSTLINK_RPC_SERVER_LOG at '{path}': {err}. \
                         Falling back to discarding ggml-rpc-server output."
                    );
                    err
                })
                .ok()
        });

    // try_clone() only fails on rare OS-level fd duplication errors; on
    // failure that stream just falls back to null rather than aborting this
    // best-effort startup path.
    let stdout_cfg = log_file
        .as_ref()
        .and_then(|f| f.try_clone().ok())
        .map(Stdio::from)
        .unwrap_or_else(Stdio::null);
    let stderr_cfg = log_file
        .as_ref()
        .and_then(|f| f.try_clone().ok())
        .map(Stdio::from)
        .unwrap_or_else(Stdio::null);

    match Command::new(&bin)
        .arg("-H")
        .arg(&spawn_host)
        .arg("-p")
        .arg(spawn_port.to_string())
        .stdout(stdout_cfg)
        .stderr(stderr_cfg)
        .stdin(Stdio::null())
        .spawn()
    {
        Ok(child) => {
            sup.pid = Some(child.id());
            sup.child = Some(child);
            sup.start_time = Some(SystemTime::now());
            sup.restart_count += 1;
            sup.consecutive_failures += 1;
            tracing::info!(
                "rpc_cluster: spawned ggml-rpc-server (PID {:?}) on {spawn_host}:{spawn_port} (restart count {})",
                sup.pid,
                sup.restart_count
            );
        }
        Err(err) => {
            sup.consecutive_failures += 1;
            sup.last_exit_status = Some(format!("Spawn failed: {}", err));
            tracing::warn!(
                "rpc_cluster: failed to start ggml-rpc-server ('{bin}'): {err}. \
                 This node will not contribute compute to the cluster."
            );
        }
    }
}

/// Stops this node's contributor process, if one is running. Not yet wired
/// to a caller (there's no live settings-toggle-off path yet — changing
/// `contribute_compute` currently takes effect on next restart, same as
/// other settings that affect process launch), kept for that follow-up and
/// for tests.
#[allow(dead_code)]
pub fn stop_contributing() {
    let handle = supervisor_handle();
    let mut sup = match handle.lock() {
        Ok(guard) => guard,
        Err(p) => p.into_inner(),
    };
    sup.stop();
}

/// Spawns the allowlist proxy exactly once per process, the first time
/// `ensure_contributing` sees a non-empty `allowed_peers`. Guarded by
/// `RPC_ALLOWLIST_PROXY_STARTED` (only the caller whose `set()` call wins
/// the race actually spawns) so the periodic respawn-supervision loop's
/// repeated `ensure_contributing` calls don't try to rebind the public port
/// every 30s.
fn maybe_start_allowlist_proxy(
    bind_host: &str,
    public_port: u16,
    internal_port: u16,
    allowed_peers: &[String],
    shared_secret: &str,
    rt_handle: &Handle,
) {
    if RPC_ALLOWLIST_PROXY_STARTED.get().is_some() {
        return;
    }

    let public_addr_str = format!("{bind_host}:{public_port}");
    let public_addr: SocketAddr = match public_addr_str.parse() {
        Ok(addr) => addr,
        Err(err) => {
            tracing::warn!(
                "rpc_cluster: cannot start rpc_allowed_peers proxy \u{2014} invalid bind address \
                 '{public_addr_str}': {err}. ggml-rpc-server will stay loopback-only and \
                 unreachable from the network until this is fixed."
            );
            return;
        }
    };
    let backend_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), internal_port);
    let allowed = allowed_peers.to_vec();
    let require_admission = !shared_secret.is_empty();

    // Only the thread whose `set()` succeeds spawns — race-free without an
    // extra Mutex, and cheap even under the (practically impossible, given
    // `ensure_contributing` already serializes callers through its own
    // `contributor_handle()` lock) case of concurrent first calls.
    if RPC_ALLOWLIST_PROXY_STARTED.set(()).is_ok() {
        rt_handle.spawn(async move {
            if let Err(err) =
                run_rpc_allowlist_proxy(public_addr, backend_addr, allowed, require_admission).await
            {
                tracing::warn!(
                    "rpc_cluster: rpc_allowed_peers proxy on {public_addr} exited with error: \
                     {err} \u{2014} the node will stop accepting distributed-inference compute \
                     jobs from any peer until this process restarts."
                );
            }
        });
    }
}

/// Binds `public_addr` and runs the allowlist proxy forever (see this
/// module's top-of-file SECURITY doc for the mechanism). Returns only on a
/// bind failure; a running proxy never returns on its own. Intended to be
/// spawned as a background task, not awaited to completion.
async fn run_rpc_allowlist_proxy(
    public_addr: SocketAddr,
    backend_addr: SocketAddr,
    allowed_peers: Vec<String>,
    require_admission: bool,
) -> std::io::Result<()> {
    let listener = TcpListener::bind(public_addr).await?;
    tracing::info!(
        "rpc_cluster: rpc_allowed_peers proxy listening on {public_addr}, forwarding allowed \
         peers to ggml-rpc-server at {backend_addr} ({} allowlist entries, require_admission={})",
        allowed_peers.len(),
        require_admission
    );
    serve_rpc_allowlist_proxy(listener, backend_addr, allowed_peers, require_admission).await
}

/// Accept loop shared by `run_rpc_allowlist_proxy` and its tests: for each
/// inbound connection, checks the peer's source IP against `allowed_peers`
/// (`ip_allowed`) and, when `require_admission` is set, also requires a
/// live `rpc_shared_secret` handshake admission (`is_admitted`) for that
/// source IP — see this module's top-of-file SECURITY doc. A connection
/// passing both checks is spliced through to `backend_addr` via
/// `tokio::io::copy_bidirectional`; anything else is dropped immediately
/// with a `tracing::warn!` naming the rejected IP and reason. Split out
/// from `run_rpc_allowlist_proxy` so tests can bind an ephemeral loopback
/// port themselves (avoiding a bind/rebind race) instead of going through a
/// fixed `SocketAddr`.
async fn serve_rpc_allowlist_proxy(
    listener: TcpListener,
    backend_addr: SocketAddr,
    allowed_peers: Vec<String>,
    require_admission: bool,
) -> std::io::Result<()> {
    loop {
        let (inbound, peer_addr) = match listener.accept().await {
            Ok(pair) => pair,
            Err(err) => {
                tracing::warn!("rpc_cluster: rpc_allowed_peers proxy accept() failed: {err}");
                continue;
            }
        };

        let ip_ok = ip_allowed(&peer_addr.ip(), &allowed_peers);
        let admitted = is_admitted(&peer_addr.ip());
        if !connection_allowed(ip_ok, require_admission, admitted) {
            tracing::warn!(
                "rpc_cluster: rejected ggml-rpc-server connection from {} \u{2014} {}",
                peer_addr.ip(),
                if !ip_ok {
                    "not in rpc_allowed_peers"
                } else {
                    "no live rpc_shared_secret admission for this source IP"
                }
            );
            continue; // dropping `inbound` closes the connection
        }

        tokio::spawn(async move {
            let mut inbound = inbound;
            match TcpStream::connect(backend_addr).await {
                Ok(mut outbound) => {
                    if let Err(err) = copy_bidirectional(&mut inbound, &mut outbound).await {
                        tracing::debug!(
                            "rpc_cluster: proxied connection from {peer_addr} ended: {err}"
                        );
                    }
                }
                Err(err) => {
                    tracing::warn!(
                        "rpc_cluster: rpc_allowed_peers proxy could not reach ggml-rpc-server at \
                         {backend_addr}: {err} \u{2014} closing connection from {peer_addr}"
                    );
                }
            }
        });
    }
}

/// Starts the RPC-auth handshake listener exactly once per process, the
/// first time `ensure_contributing` sees a non-empty `secret` — mirrors
/// `maybe_start_allowlist_proxy`'s `OnceLock`-guarded, `rt_handle`-spawned
/// pattern for the same reason (this function isn't async, may run before
/// the tokio runtime's `block_on` starts, and the periodic 30s
/// respawn-supervision loop must not try to rebind every call).
static RPC_AUTH_PORT_STARTED: OnceLock<()> = OnceLock::new();

fn maybe_start_auth_port(bind_host: &str, rpc_port: u16, secret: &str, rt_handle: &Handle) {
    if secret.is_empty() || RPC_AUTH_PORT_STARTED.get().is_some() {
        return;
    }

    let auth_port = rpc_port.saturating_add(RPC_AUTH_PORT_OFFSET);
    let addr_str = format!("{bind_host}:{auth_port}");
    let addr: SocketAddr = match addr_str.parse() {
        Ok(addr) => addr,
        Err(err) => {
            tracing::warn!(
                "rpc_cluster: cannot start the RPC auth-port listener \u{2014} invalid bind \
                 address '{addr_str}': {err}. rpc_shared_secret admission will never succeed \
                 until this is fixed."
            );
            return;
        }
    };
    let secret = secret.to_string();

    if RPC_AUTH_PORT_STARTED.set(()).is_ok() {
        rt_handle.spawn(async move {
            if let Err(err) = run_rpc_auth_port(addr, secret).await {
                tracing::warn!(
                    "rpc_cluster: RPC auth-port listener on {addr} exited with error: {err} \
                     \u{2014} this node will stop admitting new rpc_shared_secret handshakes \
                     until it restarts."
                );
            }
        });
    }
}

/// Binds `addr` and runs the auth-port accept loop forever. Returns only on
/// a bind failure. Intended to be spawned as a background task.
async fn run_rpc_auth_port(addr: SocketAddr, secret: String) -> std::io::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    tracing::info!(
        "rpc_cluster: RPC auth-port listener on {addr} (rpc_shared_secret configured, \
         RPC_ADMISSION_TTL={RPC_ADMISSION_TTL:?})"
    );
    serve_rpc_auth_port(listener, secret).await
}

/// Accept loop shared by `run_rpc_auth_port` and its tests — mirrors
/// `serve_rpc_allowlist_proxy`'s split for the same reason (tests bind an
/// ephemeral loopback port themselves, avoiding a bind/rebind race).
async fn serve_rpc_auth_port(listener: TcpListener, secret: String) -> std::io::Result<()> {
    loop {
        let (stream, peer_addr) = match listener.accept().await {
            Ok(pair) => pair,
            Err(err) => {
                tracing::warn!("rpc_cluster: RPC auth-port accept() failed: {err}");
                continue;
            }
        };
        let secret = secret.clone();
        tokio::spawn(async move {
            match handle_auth_handshake(stream, &secret).await {
                Ok(true) => {
                    admit_peer(peer_addr.ip());
                    tracing::info!("rpc_cluster: admitted RPC peer {}", peer_addr.ip());
                }
                Ok(false) => {
                    tracing::warn!(
                        "rpc_cluster: rejected RPC auth-port handshake from {} \u{2014} MAC did \
                         not match (wrong or missing rpc_shared_secret on the connecting side)",
                        peer_addr.ip()
                    );
                }
                Err(err) => {
                    tracing::warn!(
                        "rpc_cluster: RPC auth-port handshake with {} failed: {err}",
                        peer_addr.ip()
                    );
                }
            }
        });
    }
}

/// One handshake, server side: send a fresh random nonce, read back the
/// client's `HMAC-SHA256(secret, nonce)`, compare. Returns `Ok(true/false)`
/// for a completed handshake (match or mismatch) so the caller can log each
/// case at the right severity; `Err` only for a real I/O or timeout
/// failure. Never logs the secret, nonce, or MAC.
async fn handle_auth_handshake(mut stream: TcpStream, secret: &str) -> std::io::Result<bool> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let nonce: [u8; RPC_NONCE_LEN] = rand::random();
    tokio::time::timeout(RPC_AUTH_HANDSHAKE_TIMEOUT, stream.write_all(&nonce))
        .await
        .map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::TimedOut, "timed out sending nonce")
        })??;

    let mut response = [0u8; RPC_MAC_LEN];
    tokio::time::timeout(RPC_AUTH_HANDSHAKE_TIMEOUT, stream.read_exact(&mut response))
        .await
        .map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::TimedOut, "timed out reading MAC")
        })??;

    let expected = compute_hmac(secret, &nonce);
    let matched = expected == response;

    let ack = [u8::from(matched)];
    // Best-effort ack — if the client already hung up, the outcome (and the
    // admission it may have already triggered) is unaffected either way.
    let _ = tokio::time::timeout(RPC_AUTH_HANDSHAKE_TIMEOUT, stream.write_all(&ack)).await;

    Ok(matched)
}

/// Attempts the RPC-auth handshake against `peer_ip`'s auth port (derived
/// from `peer_rpc_port`), proving this node holds `secret`. Best-effort:
/// any failure (peer doesn't have the auth port running — no
/// `rpc_shared_secret` configured there, wrong secret, timeout) returns
/// `false` rather than propagating an error, matching
/// `discover_rpc_peers`'s existing "warn and exclude this peer, don't fail
/// the whole request" pattern for build-mismatch/low-health peers.
pub async fn admit_via_secret(peer_ip: IpAddr, peer_rpc_port: u16, secret: &str) -> bool {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let auth_addr = SocketAddr::new(peer_ip, peer_rpc_port.saturating_add(RPC_AUTH_PORT_OFFSET));

    let attempt = async {
        let mut stream = TcpStream::connect(auth_addr).await?;
        let mut nonce = [u8::default(); RPC_NONCE_LEN];
        stream.read_exact(&mut nonce).await?;
        let mac = compute_hmac(secret, &nonce);
        stream.write_all(&mac).await?;
        let mut ack = [0u8; 1];
        stream.read_exact(&mut ack).await?;
        Ok::<bool, std::io::Error>(ack[0] == 1)
    };

    match tokio::time::timeout(RPC_AUTH_CLIENT_TIMEOUT, attempt).await {
        Ok(Ok(true)) => true,
        Ok(Ok(false)) => {
            tracing::warn!(
                "rpc_cluster: RPC peer {peer_ip} rejected our rpc_shared_secret \u{2014} \
                 excluding it from this distributed-inference request"
            );
            false
        }
        Ok(Err(err)) => {
            tracing::warn!(
                "rpc_cluster: could not complete RPC auth handshake with {peer_ip}: {err} \
                 \u{2014} excluding it from this distributed-inference request"
            );
            false
        }
        Err(_) => {
            tracing::warn!(
                "rpc_cluster: RPC auth handshake with {peer_ip} timed out after \
                 {RPC_AUTH_CLIENT_TIMEOUT:?} \u{2014} excluding it from this \
                 distributed-inference request"
            );
            false
        }
    }
}

/// Checks `ip` against `allowlist` (settings.json's `rpc_allowed_peers`).
/// Empty allowlist means "allow all" — the default-open behavior. Each
/// entry is either an exact IPv4 address ("10.0.0.29") or IPv4 CIDR
/// ("10.0.0.0/24"); no CIDR-matching crate (e.g. `ipnet`, which appears in
/// `Cargo.lock` only as a transitive dependency of unrelated crates, not a
/// direct one) was already a dependency of this workspace, so this is a
/// small hand-written IPv4-only matcher rather than a new direct
/// dependency for a narrowly-scoped need.
///
/// IPv6 is not supported: an IPv6 `ip` is rejected (logged, not matched
/// against anything) rather than silently mismatched against IPv4 entries,
/// and a malformed or IPv6 allowlist entry is logged and skipped rather
/// than panicking or being silently ignored without a trace.
pub fn ip_allowed(ip: &IpAddr, allowlist: &[String]) -> bool {
    if allowlist.is_empty() {
        return true;
    }

    let ip_v4 = match ip {
        IpAddr::V4(v4) => *v4,
        IpAddr::V6(_) => {
            tracing::warn!(
                "rpc_cluster: rejecting connection from {ip} \u{2014} rpc_allowed_peers only \
                 supports IPv4 addresses/CIDR ranges today, so an IPv6 peer can never match"
            );
            return false;
        }
    };

    allowlist
        .iter()
        .any(|entry| ipv4_entry_matches(entry, ip_v4))
}

/// Matches a single `rpc_allowed_peers` entry (exact IPv4 address or IPv4
/// CIDR) against `ip`. Malformed entries are logged and treated as
/// non-matching rather than panicking — a typo in one allowlist entry
/// should not take down the whole allowlist check.
fn ipv4_entry_matches(entry: &str, ip: Ipv4Addr) -> bool {
    let entry = entry.trim();

    let Some((network_str, prefix_str)) = entry.split_once('/') else {
        return match entry.parse::<Ipv4Addr>() {
            Ok(addr) => addr == ip,
            Err(_) => {
                tracing::warn!(
                    "rpc_cluster: rpc_allowed_peers entry '{entry}' is not a valid IPv4 address \
                     \u{2014} ignoring it"
                );
                false
            }
        };
    };

    let (Ok(network), Ok(prefix)) = (network_str.parse::<Ipv4Addr>(), prefix_str.parse::<u32>())
    else {
        tracing::warn!(
            "rpc_cluster: rpc_allowed_peers entry '{entry}' is not a valid IPv4 CIDR range \
             \u{2014} ignoring it"
        );
        return false;
    };
    if prefix > 32 {
        tracing::warn!(
            "rpc_cluster: rpc_allowed_peers entry '{entry}' has an invalid CIDR prefix length \
             (must be 0-32) \u{2014} ignoring it"
        );
        return false;
    }

    let mask: u32 = if prefix == 0 {
        0
    } else {
        u32::MAX << (32 - prefix)
    };
    (u32::from(network) & mask) == (u32::from(ip) & mask)
}

/// A cluster peer known to be contributing compute over `ggml-rpc-server`.
#[derive(Debug, Clone, PartialEq)]
pub struct RpcPeer {
    pub node_id: String,
    pub addr: SocketAddr,
    pub vram_gb: f32,
    /// System RAM in GB, used as a `compute_tensor_split` fallback signal
    /// when this peer has no VRAM to report (CPU-only node).
    pub system_memory_gb: f32,
}

/// Finds healthy, RPC-contributing peers (excluding `local_node_id`) from
/// `cluster`'s live node list. A peer only qualifies if it advertised an
/// `rpc_port` (opted in to contributing), is `NodeStatus::Active`, has a
/// known reachable IP (from discovery or `/api/workers/connect`), and
/// doesn't have a *confirmed* `llama.cpp` build mismatch against
/// `local_rpc_build_id`. Sorted by node id for a stable
/// `--rpc`/`--tensor-split` ordering across a session.
///
/// # Version-compatibility check
///
/// `ggml-rpc-server` has no version-compatibility check of its own — an
/// upstream llama.cpp limitation. Confirmed on real, genuinely separate
/// hardware (`docs/BENCHMARKS.md`'s native two-machine entry): two machines
/// running `llama.cpp` builds 10 days apart connected and exchanged RPC
/// traffic without complaint, but larger-model inference through the real
/// distributed path came back as reproducible garbage while the API
/// reported `healthy`/HTTP 200 throughout. Rebuilding both sides at the
/// identical commit fixed it.
///
/// A peer is only excluded on an *actual confirmed* mismatch — both this
/// node and the peer report a `rpc_build_id` and they differ. A peer
/// reporting `None` (it predates this field, or couldn't determine its own
/// build id) is deliberately **not** treated as a mismatch: that would make
/// every not-yet-updated peer on a LAN unroutable the moment one node
/// upgrades, an unnecessarily disruptive rollout behavior. Instead this
/// ships additively: unknown-version peers are still used, with a lower-
/// urgency warning that their safety can't be verified — matching today's
/// actual (pre-this-field) behavior exactly.
/// Detailed peer admission and exclusion status for the Phase 4 Cluster Map GUI.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PeerEvaluation {
    pub rpc_port: Option<u16>,
    pub contribute_compute: bool,
    pub build_id_status: String,
    pub secret_status: String,
    pub allowlist_status: String,
    pub role: String,
    pub excluded_reason: Option<String>,
}

/// Evaluates a peer's detailed status and exclusion reason relative to local coordinator settings.
pub fn evaluate_peer(
    node: &ghostlink_core::protocol::NodeResources,
    metrics: Option<&ghostlink_core::cluster::NodeMetrics>,
    local_rpc_build_id: Option<&str>,
    local_secret_configured: bool,
    allowlist: &[String],
    is_local: bool,
    is_used_in_rpc: bool,
) -> PeerEvaluation {
    if is_local {
        let healthy = is_contributing_healthy();
        let contribute_effective = node.rpc_port.is_some() && healthy;
        let reason = if node.rpc_port.is_some() && !healthy {
            Some("rpc child not running".to_string())
        } else {
            None
        };
        return PeerEvaluation {
            rpc_port: if contribute_effective {
                node.rpc_port
            } else {
                None
            },
            contribute_compute: contribute_effective,
            build_id_status: "match".to_string(),
            secret_status: if local_secret_configured {
                "match".to_string()
            } else {
                "n/a".to_string()
            },
            allowlist_status: "n/a".to_string(),
            role: "coordinator".to_string(),
            excluded_reason: reason,
        };
    }

    let rpc_port = node.rpc_port;
    let contribute_compute = rpc_port.is_some();
    let ip = metrics.and_then(|m| m.ip_address).map(|a| a.ip());

    let build_id_status = match (local_rpc_build_id, node.rpc_build_id.as_deref()) {
        (Some(local), Some(peer)) => {
            if local == peer {
                "match".to_string()
            } else {
                "mismatch".to_string()
            }
        }
        _ => "unknown".to_string(),
    };

    let allowlist_status = if allowlist.is_empty() {
        "n/a".to_string()
    } else if let Some(ref peer_ip) = ip {
        if ip_allowed(peer_ip, allowlist) {
            "allowed".to_string()
        } else {
            "blocked".to_string()
        }
    } else {
        "n/a".to_string()
    };

    let secret_status = if !local_secret_configured {
        "n/a".to_string()
    } else if let Some(ref peer_ip) = ip {
        if is_admitted(peer_ip) {
            "match".to_string()
        } else {
            "missing/mismatch".to_string()
        }
    } else {
        "missing/mismatch".to_string()
    };

    let mut reason: Option<String> = None;

    if !contribute_compute {
        reason = Some("contribute_compute off / no rpc_port advertised".to_string());
    } else if metrics.is_none_or(|m| m.status != NodeStatus::Active) {
        reason = Some("unhealthy / stale heartbeat".to_string());
    } else if metrics.is_some_and(|m| m.delivery_ratio < RPC_PEER_MIN_DELIVERY_RATIO) {
        reason = Some("unhealthy / low packet delivery ratio".to_string());
    } else if allowlist_status == "blocked" {
        reason = Some("peer IP not in coordinator rpc_allowed_peers".to_string());
    } else if secret_status == "missing/mismatch" {
        reason = Some("rpc_shared_secret missing or handshake mismatch".to_string());
    } else if build_id_status == "mismatch" {
        reason = Some("RPC build does not match coordinator".to_string());
    }

    let role = if is_used_in_rpc {
        "contributor".to_string()
    } else {
        "unused".to_string()
    };

    PeerEvaluation {
        rpc_port,
        contribute_compute,
        build_id_status,
        secret_status,
        allowlist_status,
        role,
        excluded_reason: reason,
    }
}

pub fn discover_rpc_peers(
    cluster: &ClusterState,
    local_node_id: &str,
    local_rpc_build_id: Option<&str>,
) -> Vec<RpcPeer> {
    let nodes = cluster.nodes_snapshot();
    cluster.with_metrics(|metrics_map| {
        let mut peers: Vec<RpcPeer> = nodes
            .iter()
            .filter(|node| node.id != local_node_id)
            .filter_map(|node| {
                let rpc_port = node.rpc_port?;
                let metrics = metrics_map.get(&node.id)?;
                if metrics.status != NodeStatus::Active {
                    return None;
                }

                // Real-time health gate: a peer that's technically `Active` (its
                // heartbeat hasn't timed out) can still be struggling badly
                // enough that routing layers to it hurts overall throughput more
                // than excluding it would. `delivery_ratio` defaults to 1.0 for
                // a peer with no samples yet, so this never penalizes a
                // freshly-joined node before it has real data.
                //
                // Deliberately NOT also gating on `metrics.avg_latency_us` here:
                // that field is named as microseconds but is populated with
                // millisecond-scale values elsewhere in this codebase (e.g.
                // `record_latency(120.0)`/`record_latency(180.0)` in main.rs),
                // and `planning::RebalanceTrigger`'s existing `> 15.0` threshold
                // for it was written for the synthetic pipeline path, not
                // measured against this real RPC heartbeat path. Copying an
                // ambiguously-scaled cutoff here risks silently excluding every
                // real peer (or none) depending on which unit actually applies
                // — worse than not gating on it at all.
                if metrics.delivery_ratio < RPC_PEER_MIN_DELIVERY_RATIO {
                    tracing::warn!(
                        "rpc_cluster: excluding peer '{}' \u{2014} delivery_ratio {:.3} is below the \
                         {RPC_PEER_MIN_DELIVERY_RATIO} health floor for distributed inference.",
                        node.id,
                        metrics.delivery_ratio,
                    );
                    return None;
                }

                match (local_rpc_build_id, node.rpc_build_id.as_deref()) {
                    (Some(local), Some(peer)) if local != peer => {
                        tracing::warn!(
                            "rpc_cluster: excluding peer '{}' ({peer_addr:?}) \u{2014} llama.cpp \
                             build mismatch (local: '{local}', peer: '{peer}'). Distributed \
                             inference is skipping this peer because version-mismatched ggml-rpc \
                             peers have been confirmed to silently corrupt output for larger models \
                             while reporting healthy status throughout. Rebuild both nodes at the \
                             same llama.cpp commit to re-enable this peer.",
                            node.id,
                            peer_addr = metrics.ip_address,
                        );
                        return None;
                    }
                    (Some(local), Some(peer)) => {
                        debug_assert_eq!(local, peer);
                    }
                    _ => {
                        tracing::warn!(
                            "rpc_cluster: peer '{}' did not report a verifiable llama.cpp build \
                             fingerprint (predates version-compatibility checking, or couldn't \
                             determine its own build id) \u{2014} cannot verify it matches this \
                             node's build; proceeding anyway.",
                            node.id
                        );
                    }
                }

                let mut addr = metrics.ip_address?;
                addr.set_port(rpc_port);
                Some(RpcPeer {
                    node_id: node.id.clone(),
                    addr,
                    vram_gb: node.vram_gb,
                    system_memory_gb: metrics.system_memory_gb,
                })
            })
            .collect();
        peers.sort_by(|a, b| a.node_id.cmp(&b.node_id));
        peers
    })
}

/// Conservative haircut constant applied to system RAM for CPU-only (0 VRAM) nodes.
/// Scaling system RAM by 0.5 (50%) prevents OOMing CPU-only nodes while allowing
/// CPU-only peers to accept a meaningful weight share when needed to fit models.
pub const CPU_RAM_HAIRCUT: f32 = 0.5;

/// Computes a `--tensor-split` ratio: the local device first, then each
/// remote peer in the same order as `peers` — llama.cpp registers backend
/// devices in exactly that order (local device(s), then `--rpc` targets in
/// the order given), so the split array's positions must line up with it.
///
/// Shares are computed based on weighted capacity:
/// - Discrete GPU nodes use reported VRAM (in GB) when VRAM > 0.
/// - CPU-only nodes (0 VRAM) use system RAM scaled by `CPU_RAM_HAIRCUT` (0.5).
/// - Every device weight is floored at 0.1 to avoid a dead device entry.
pub fn compute_tensor_split(
    local_vram_gb: f32,
    local_system_memory_gb: f32,
    peers: &[RpcPeer],
) -> Vec<f32> {
    let calc_weight = |vram: f32, ram: f32| -> f32 {
        if vram > 0.0 {
            vram.max(0.1)
        } else {
            (ram * CPU_RAM_HAIRCUT).max(0.1)
        }
    };

    let mut split = vec![calc_weight(local_vram_gb, local_system_memory_gb)];
    split.extend(
        peers
            .iter()
            .map(|p| calc_weight(p.vram_gb, p.system_memory_gb)),
    );
    split
}

/// Formats the `--rpc` flag value from a peer list (`host:port,host:port`).
pub fn rpc_flag_value(peers: &[RpcPeer]) -> String {
    peers
        .iter()
        .map(|p| p.addr.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

/// Formats the `-ts`/`--tensor-split` flag value.
pub fn tensor_split_flag_value(split: &[f32]) -> String {
    split
        .iter()
        .map(|v| format!("{v:.4}"))
        .collect::<Vec<_>>()
        .join(",")
}

/// Minimum required aggregate remote split share (1%) for a load to be treated as distributed.
pub const MIN_REMOTE_SHARE_THRESHOLD: f32 = 0.01;

/// Validates whether a computed tensor split and effective -ngl qualify for distributed offload.
/// Returns Ok(()) if valid, or Err(warning_reason) if invalid.
pub fn validate_distributed_offload(ngl: i32, tensor_splits: &[f32]) -> Result<(), String> {
    if ngl == 0 {
        return Err("distributed_inference is enabled but effective -ngl is 0 (CPU-only), so no layers leave the coordinator".to_string());
    }
    let total_weight: f32 = tensor_splits.iter().sum();
    if total_weight <= 0.0 {
        return Err("distributed_inference tensor split total weight is zero".to_string());
    }
    let remote_weight: f32 = tensor_splits.iter().skip(1).sum();
    let remote_share = remote_weight / total_weight;
    if remote_share < MIN_REMOTE_SHARE_THRESHOLD {
        return Err(format!(
            "remote tensor share ({:.4}%) is below the {:.1}% minimum threshold for distributed offload",
            remote_share * 100.0,
            MIN_REMOTE_SHARE_THRESHOLD * 100.0
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ghostlink_core::protocol::NodeResources;
    use std::net::{IpAddr, Ipv4Addr};

    fn register_peer(
        cluster: &ClusterState,
        id: &str,
        vram_gb: f32,
        rpc_port: Option<u16>,
        addr: Option<SocketAddr>,
        status: NodeStatus,
    ) {
        register_peer_with_build_id(cluster, id, vram_gb, rpc_port, None, addr, status);
    }

    #[allow(clippy::too_many_arguments)]
    fn register_peer_with_build_id(
        cluster: &ClusterState,
        id: &str,
        vram_gb: f32,
        rpc_port: Option<u16>,
        rpc_build_id: Option<&str>,
        addr: Option<SocketAddr>,
        status: NodeStatus,
    ) {
        let mut node = NodeResources::new(id, vram_gb, 32.0, "8.6", None);
        if let Some(port) = rpc_port {
            node = node.with_rpc_port(port);
        }
        if let Some(build_id) = rpc_build_id {
            node = node.with_rpc_build_id(build_id);
        }
        cluster.register_with_addr(node, addr);
        cluster.get_metrics_mut(id, |m| m.status = status);
    }

    fn loopback(port: u16) -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port)
    }

    #[test]
    fn discovers_only_healthy_rpc_contributing_peers() {
        let cluster = ClusterState::new();
        register_peer(
            &cluster,
            "local",
            16.0,
            None,
            Some(loopback(9000)),
            NodeStatus::Active,
        );
        register_peer_with_build_id(
            &cluster,
            "contributor",
            12.0,
            Some(50052),
            None,
            Some(loopback(9001)),
            NodeStatus::Active,
        );
        register_peer(
            &cluster,
            "no-rpc",
            8.0,
            None,
            Some(loopback(9002)),
            NodeStatus::Active,
        );
        register_peer(
            &cluster,
            "unhealthy",
            8.0,
            Some(50053),
            Some(loopback(9003)),
            NodeStatus::Failed,
        );
        register_peer(
            &cluster,
            "no-addr",
            8.0,
            Some(50054),
            None,
            NodeStatus::Active,
        );

        let peers = discover_rpc_peers(&cluster, "local", None);

        assert_eq!(peers.len(), 1);
        assert_eq!(peers[0].node_id, "contributor");
        assert_eq!(peers[0].addr, loopback(50052));
        assert_eq!(peers[0].vram_gb, 12.0);
    }

    #[test]
    fn excludes_peer_with_low_delivery_ratio_even_if_status_is_active() {
        let cluster = ClusterState::new();
        register_peer(
            &cluster,
            "flaky",
            12.0,
            Some(50052),
            Some(loopback(9001)),
            NodeStatus::Active,
        );
        // Below the heartbeat-timeout threshold that would flip status to
        // Failed, but bad enough that routing real inference layers here
        // would hurt throughput more than excluding it.
        cluster.get_metrics_mut("flaky", |m| m.delivery_ratio = 0.5);

        let peers = discover_rpc_peers(&cluster, "local", None);

        assert!(peers.is_empty());
    }

    #[test]
    fn keeps_fresh_peer_with_no_delivery_samples_yet() {
        let cluster = ClusterState::new();
        register_peer(
            &cluster,
            "fresh",
            12.0,
            Some(50052),
            Some(loopback(9001)),
            NodeStatus::Active,
        );
        // delivery_ratio defaults to 1.0 until real samples come in — must
        // not be treated as "below the health floor" before it has data.

        let peers = discover_rpc_peers(&cluster, "local", None);

        assert_eq!(peers.len(), 1);
        assert_eq!(peers[0].node_id, "fresh");
    }

    #[test]
    fn excludes_local_node_even_if_self_advertises_rpc_port() {
        let cluster = ClusterState::new();
        register_peer(
            &cluster,
            "local",
            16.0,
            Some(50052),
            Some(loopback(9000)),
            NodeStatus::Active,
        );

        assert!(discover_rpc_peers(&cluster, "local", None).is_empty());
    }

    #[test]
    fn peer_order_is_stable_and_sorted_by_id() {
        let cluster = ClusterState::new();
        register_peer(
            &cluster,
            "zeta",
            8.0,
            Some(50052),
            Some(loopback(9001)),
            NodeStatus::Active,
        );
        register_peer(
            &cluster,
            "alpha",
            8.0,
            Some(50053),
            Some(loopback(9002)),
            NodeStatus::Active,
        );

        let peers = discover_rpc_peers(&cluster, "local", None);
        assert_eq!(peers.len(), 2);
        assert_eq!(peers[0].node_id, "alpha");
        assert_eq!(peers[1].node_id, "zeta");
    }

    #[test]
    fn excludes_peer_with_confirmed_mismatched_rpc_build_id() {
        let cluster = ClusterState::new();
        register_peer_with_build_id(
            &cluster,
            "local",
            16.0,
            None,
            None,
            Some(loopback(9000)),
            NodeStatus::Active,
        );
        register_peer_with_build_id(
            &cluster,
            "contributor",
            12.0,
            Some(50052),
            Some("e920c523e"),
            Some(loopback(9001)),
            NodeStatus::Active,
        );

        let peers = discover_rpc_peers(&cluster, "local", Some("da296d6"));

        assert!(
            peers.is_empty(),
            "peer with a confirmed different llama.cpp build id must be excluded"
        );
    }

    #[test]
    fn does_not_exclude_peer_with_matching_rpc_build_id() {
        let cluster = ClusterState::new();
        register_peer_with_build_id(
            &cluster,
            "local",
            16.0,
            None,
            None,
            Some(loopback(9000)),
            NodeStatus::Active,
        );
        register_peer_with_build_id(
            &cluster,
            "contributor",
            12.0,
            Some(50052),
            Some("da296d6"),
            Some(loopback(9001)),
            NodeStatus::Active,
        );

        let peers = discover_rpc_peers(&cluster, "local", Some("da296d6"));

        assert_eq!(peers.len(), 1);
        assert_eq!(peers[0].node_id, "contributor");
    }

    #[test]
    fn does_not_exclude_peer_with_unknown_rpc_build_id() {
        // Asymmetry from bug 1's fix: a peer reporting `None` (predates this
        // field, or couldn't determine its own build) must NOT be treated as
        // a mismatch and excluded -- only an *actual confirmed* mismatch
        // (Some(a) != Some(b)) should exclude a peer. Otherwise every
        // not-yet-updated peer on a LAN becomes unroutable the moment one
        // node upgrades.
        let cluster = ClusterState::new();
        register_peer_with_build_id(
            &cluster,
            "local",
            16.0,
            None,
            None,
            Some(loopback(9000)),
            NodeStatus::Active,
        );
        register_peer_with_build_id(
            &cluster,
            "contributor",
            12.0,
            Some(50052),
            None,
            Some(loopback(9001)),
            NodeStatus::Active,
        );

        let peers = discover_rpc_peers(&cluster, "local", Some("da296d6"));

        assert_eq!(
            peers.len(),
            1,
            "a peer with no rpc_build_id must still be used, not excluded"
        );
        assert_eq!(peers[0].node_id, "contributor");
    }

    #[test]
    fn does_not_exclude_peer_when_local_build_id_is_unknown() {
        // Mirror case: if this node itself couldn't determine its own build
        // id, it also can't confirm a mismatch, so peers should still be used.
        let cluster = ClusterState::new();
        register_peer_with_build_id(
            &cluster,
            "local",
            16.0,
            None,
            None,
            Some(loopback(9000)),
            NodeStatus::Active,
        );
        register_peer_with_build_id(
            &cluster,
            "contributor",
            12.0,
            Some(50052),
            Some("e920c523e"),
            Some(loopback(9001)),
            NodeStatus::Active,
        );

        let peers = discover_rpc_peers(&cluster, "local", None);

        assert_eq!(peers.len(), 1);
        assert_eq!(peers[0].node_id, "contributor");
    }

    #[test]
    fn tensor_split_puts_local_first_proportional_to_vram() {
        let peers = vec![
            RpcPeer {
                node_id: "a".to_string(),
                addr: loopback(1),
                vram_gb: 12.0,
                system_memory_gb: 32.0,
            },
            RpcPeer {
                node_id: "b".to_string(),
                addr: loopback(2),
                vram_gb: 8.0,
                system_memory_gb: 16.0,
            },
        ];
        let split = compute_tensor_split(16.0, 64.0, &peers);
        assert_eq!(split, vec![16.0, 12.0, 8.0]);
    }

    #[test]
    fn tensor_split_floors_zero_vram_to_avoid_a_dead_device_entry() {
        let split = compute_tensor_split(0.0, 0.0, &[]);
        assert_eq!(split, vec![0.1]);
    }

    #[test]
    fn tensor_split_two_gpus() {
        let peers = vec![RpcPeer {
            node_id: "peer-gpu".to_string(),
            addr: loopback(1),
            vram_gb: 12.0,
            system_memory_gb: 32.0,
        }];
        let split = compute_tensor_split(16.0, 64.0, &peers);
        assert_eq!(split, vec![16.0, 12.0]);
    }

    #[test]
    fn tensor_split_gpu_plus_zero_vram_high_ram() {
        // Coordinator GPU (16 GB VRAM), peer 0 VRAM high RAM (64 GB system RAM).
        // Peer RAM is scaled by CPU_RAM_HAIRCUT (0.5), yielding weight 32.0.
        let peers = vec![RpcPeer {
            node_id: "peer-cpu".to_string(),
            addr: loopback(1),
            vram_gb: 0.0,
            system_memory_gb: 64.0,
        }];
        let split = compute_tensor_split(16.0, 32.0, &peers);
        assert_eq!(split, vec![16.0, 32.0]);
    }

    #[test]
    fn tensor_split_two_zero_vram_nodes() {
        // Both nodes are 0 VRAM CPU-only (Coordinator 64 GB RAM, Peer 32 GB RAM).
        // Both scaled by CPU_RAM_HAIRCUT (0.5): 32.0 and 16.0.
        let peers = vec![RpcPeer {
            node_id: "peer-cpu".to_string(),
            addr: loopback(1),
            vram_gb: 0.0,
            system_memory_gb: 32.0,
        }];
        let split = compute_tensor_split(0.0, 64.0, &peers);
        assert_eq!(split, vec![32.0, 16.0]);
    }

    #[test]
    fn tensor_split_model_larger_than_coordinator_vram() {
        // Model requires e.g. 24 GB, but coordinator only has 8 GB VRAM.
        // Peer CPU node has 64 GB RAM (effective weight 32 GB).
        // Total cluster weight = 40.0, coordinator share = 8/40 = 20%, peer share = 32/40 = 80%.
        let peers = vec![RpcPeer {
            node_id: "peer-cpu-large".to_string(),
            addr: loopback(1),
            vram_gb: 0.0,
            system_memory_gb: 64.0,
        }];
        let split = compute_tensor_split(8.0, 16.0, &peers);
        assert_eq!(split, vec![8.0, 32.0]);
        let total_weight: f32 = split.iter().sum();
        let peer_share = split[1] / total_weight;
        assert_eq!(peer_share, 0.8);
    }

    #[test]
    fn rpc_flag_value_formats_host_port_list() {
        let peers = vec![
            RpcPeer {
                node_id: "a".to_string(),
                addr: loopback(50052),
                vram_gb: 8.0,
                system_memory_gb: 16.0,
            },
            RpcPeer {
                node_id: "b".to_string(),
                addr: loopback(50053),
                vram_gb: 8.0,
                system_memory_gb: 16.0,
            },
        ];
        assert_eq!(rpc_flag_value(&peers), "127.0.0.1:50052,127.0.0.1:50053");
    }

    #[test]
    fn tensor_split_flag_value_formats_comma_separated_ratios() {
        assert_eq!(
            tensor_split_flag_value(&[16.0, 12.0, 8.0]),
            "16.0000,12.0000,8.0000"
        );
    }

    #[test]
    fn rpc_flag_value_empty_for_no_peers() {
        assert_eq!(rpc_flag_value(&[]), "");
    }

    // --- ip_allowed / CIDR matching ---

    fn v4(a: u8, b: u8, c: u8, d: u8) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(a, b, c, d))
    }

    #[test]
    fn ip_allowed_empty_allowlist_allows_everything() {
        // Empty rpc_allowed_peers means "allow all" -- the default-open
        // behavior that must be unchanged from before this field existed.
        assert!(ip_allowed(&v4(10, 0, 0, 29), &[]));
        assert!(ip_allowed(&v4(203, 0, 113, 5), &[]));
    }

    #[test]
    fn ip_allowed_exact_ipv4_match() {
        let allowlist = vec!["10.0.0.29".to_string()];
        assert!(ip_allowed(&v4(10, 0, 0, 29), &allowlist));
        assert!(!ip_allowed(&v4(10, 0, 0, 30), &allowlist));
    }

    #[test]
    fn ip_allowed_cidr_range_match() {
        let allowlist = vec!["10.0.0.0/24".to_string()];
        assert!(ip_allowed(&v4(10, 0, 0, 1), &allowlist));
        assert!(ip_allowed(&v4(10, 0, 0, 254), &allowlist));
        assert!(!ip_allowed(&v4(10, 0, 1, 1), &allowlist));
        assert!(!ip_allowed(&v4(11, 0, 0, 1), &allowlist));
    }

    #[test]
    fn ip_allowed_cidr_slash_32_is_exact_match() {
        let allowlist = vec!["192.168.1.5/32".to_string()];
        assert!(ip_allowed(&v4(192, 168, 1, 5), &allowlist));
        assert!(!ip_allowed(&v4(192, 168, 1, 6), &allowlist));
    }

    #[test]
    fn ip_allowed_cidr_slash_0_matches_everything() {
        let allowlist = vec!["0.0.0.0/0".to_string()];
        assert!(ip_allowed(&v4(1, 2, 3, 4), &allowlist));
        assert!(ip_allowed(&v4(255, 255, 255, 255), &allowlist));
    }

    #[test]
    fn ip_allowed_multiple_entries_any_match_wins() {
        let allowlist = vec!["10.0.0.29".to_string(), "192.168.0.0/16".to_string()];
        assert!(ip_allowed(&v4(10, 0, 0, 29), &allowlist));
        assert!(ip_allowed(&v4(192, 168, 50, 7), &allowlist));
        assert!(!ip_allowed(&v4(172, 16, 0, 1), &allowlist));
    }

    #[test]
    fn ip_allowed_ipv6_input_rejected_not_panicking() {
        let allowlist = vec!["10.0.0.0/24".to_string()];
        let ipv6 = IpAddr::V6(std::net::Ipv6Addr::LOCALHOST);
        // Must not panic, and must not silently "match" -- an IPv6 peer is
        // always rejected against an IPv4-only allowlist.
        assert!(!ip_allowed(&ipv6, &allowlist));
    }

    #[test]
    fn ip_allowed_malformed_entry_is_skipped_not_panicking() {
        let allowlist = vec![
            "not-an-ip".to_string(),
            "10.0.0.0/99".to_string(), // invalid prefix length
            "10.0.0.29".to_string(),   // still valid, still matches
        ];
        assert!(ip_allowed(&v4(10, 0, 0, 29), &allowlist));
        assert!(!ip_allowed(&v4(10, 0, 0, 30), &allowlist));
    }

    // --- allowlist proxy (integration-style, real loopback sockets) ---

    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// Spawns a trivial echo server on an ephemeral loopback port and
    /// returns its address, standing in for the loopback `ggml-rpc-server`
    /// the proxy would normally forward to.
    async fn spawn_echo_backend() -> SocketAddr {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let (mut sock, _) = match listener.accept().await {
                    Ok(pair) => pair,
                    Err(_) => return,
                };
                tokio::spawn(async move {
                    let mut buf = [0u8; 1024];
                    loop {
                        match sock.read(&mut buf).await {
                            Ok(0) | Err(_) => return,
                            Ok(n) => {
                                if sock.write_all(&buf[..n]).await.is_err() {
                                    return;
                                }
                            }
                        }
                    }
                });
            }
        });
        addr
    }

    #[tokio::test]
    async fn allowlist_proxy_forwards_allowed_source() {
        let backend_addr = spawn_echo_backend().await;

        // Bind the proxy's public-facing listener on an ephemeral loopback
        // port ourselves (rather than a fixed SocketAddr) so there's no
        // bind/rebind race with `run_rpc_allowlist_proxy`.
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let proxy_addr = listener.local_addr().unwrap();
        let allowed_peers = vec!["127.0.0.1".to_string()];
        tokio::spawn(serve_rpc_allowlist_proxy(
            listener,
            backend_addr,
            allowed_peers,
            false,
        ));

        let mut client = TcpStream::connect(proxy_addr).await.unwrap();
        client.write_all(b"hello").await.unwrap();
        let mut resp = [0u8; 5];
        client.read_exact(&mut resp).await.unwrap();
        assert_eq!(
            &resp, b"hello",
            "allowed peer's traffic must be spliced through"
        );
    }

    #[tokio::test]
    async fn allowlist_proxy_rejects_disallowed_source() {
        let backend_addr = spawn_echo_backend().await;

        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let proxy_addr = listener.local_addr().unwrap();
        // 127.0.0.1 (what this test connects from) is deliberately NOT in
        // this allowlist.
        let allowed_peers = vec!["10.0.0.1".to_string()];
        tokio::spawn(serve_rpc_allowlist_proxy(
            listener,
            backend_addr,
            allowed_peers,
            false,
        ));

        let mut client = TcpStream::connect(proxy_addr).await.unwrap();
        let _ = client.write_all(b"hello").await; // may or may not land before close
        let mut resp = [0u8; 1];
        let n = client.read(&mut resp).await.unwrap_or(0);
        assert_eq!(
            n, 0,
            "disallowed peer's connection must be closed with no data forwarded"
        );
    }

    #[test]
    fn connection_allowed_requires_both_ip_allowlist_and_admission_when_required() {
        // Deliberately a pure test against `connection_allowed`, not a real
        // socket against the process-global admissions registry: every
        // loopback-socket test in this suite shares the same source IP
        // (127.0.0.1), so asserting "not admitted" from a real connection
        // is order-dependent on whatever else in the suite has already
        // called `admit_peer` for that same IP.
        assert!(
            connection_allowed(true, false, false),
            "allowlisted, no admission required at all"
        );
        assert!(
            !connection_allowed(true, true, false),
            "allowlisted but admission required and not yet granted"
        );
        assert!(
            connection_allowed(true, true, true),
            "allowlisted, admission required and granted"
        );
        assert!(
            !connection_allowed(false, false, false),
            "not on the allowlist at all, regardless of admission"
        );
        assert!(
            !connection_allowed(false, true, true),
            "admitted but not on the allowlist \u{2014} both checks must pass"
        );
    }

    #[tokio::test]
    async fn allowlist_proxy_forwards_allowlisted_ip_once_admitted() {
        let backend_addr = spawn_echo_backend().await;

        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let proxy_addr = listener.local_addr().unwrap();
        let allowed_peers = vec!["127.0.0.1".to_string()];
        admit_peer(IpAddr::V4(Ipv4Addr::LOCALHOST));
        tokio::spawn(serve_rpc_allowlist_proxy(
            listener,
            backend_addr,
            allowed_peers,
            true,
        ));

        let mut client = TcpStream::connect(proxy_addr).await.unwrap();
        client.write_all(b"hello").await.unwrap();
        let mut resp = [0u8; 5];
        client.read_exact(&mut resp).await.unwrap();
        assert_eq!(
            &resp, b"hello",
            "a previously-admitted, allowlisted source must be spliced through"
        );
    }

    #[test]
    fn admission_is_valid_true_before_expiry_false_at_and_after() {
        let base = Instant::now();
        let expiry = base + Duration::from_millis(50);
        assert!(admission_is_valid(expiry, base));
        assert!(!admission_is_valid(expiry, expiry));
        assert!(!admission_is_valid(
            expiry,
            expiry + Duration::from_millis(1)
        ));
    }

    #[test]
    fn admit_peer_then_is_admitted_round_trips_for_a_fresh_ip() {
        // A distinct, unlikely-to-collide-with-other-tests address, since
        // the admissions registry is process-global (OnceLock) like
        // RPC_CONTRIBUTOR_PROCESS elsewhere in this file.
        let ip = v4(198, 51, 100, 77);
        assert!(!is_admitted(&ip), "must not be admitted before admit_peer");
        admit_peer(ip);
        assert!(
            is_admitted(&ip),
            "must be admitted immediately after admit_peer"
        );
    }

    #[test]
    fn is_admitted_false_for_an_ip_that_was_never_admitted() {
        let ip = v4(198, 51, 100, 78);
        assert!(!is_admitted(&ip));
    }

    #[test]
    fn compute_hmac_is_deterministic_and_distinguishes_secret_and_nonce() {
        let nonce: [u8; RPC_NONCE_LEN] = std::array::from_fn(|idx| (idx as u8) ^ 0xA5);
        let a = compute_hmac("secret-a", &nonce);
        let b = compute_hmac("secret-a", &nonce);
        let c = compute_hmac("secret-b", &nonce);
        let other_nonce: [u8; RPC_NONCE_LEN] =
            std::array::from_fn(|idx| (idx as u8).wrapping_add(0x33));
        let d = compute_hmac("secret-a", &other_nonce);

        assert_eq!(a, b, "same secret + nonce must produce the same MAC");
        assert_ne!(a, c, "different secret must change the MAC");
        assert_ne!(a, d, "different nonce must change the MAC");
        assert_eq!(a.len(), RPC_MAC_LEN);
    }

    #[tokio::test]
    async fn auth_handshake_admits_correct_secret_and_rejects_wrong_secret() {
        // Talks to `serve_rpc_auth_port` directly via the same raw
        // handshake primitives `admit_via_secret` uses internally, rather
        // than through `admit_via_secret` itself — that function derives
        // the auth port from `peer_rpc_port + RPC_AUTH_PORT_OFFSET`, and an
        // ephemeral (`:0`) bind here has no such fixed relationship to
        // exercise. `admit_via_secret_applies_the_auth_port_offset_and_admits_on_match`
        // below covers that offset math end-to-end instead.
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let auth_addr = listener.local_addr().unwrap();
        tokio::spawn(serve_rpc_auth_port(listener, "correct-horse".to_string()));

        let mut client = TcpStream::connect(auth_addr).await.unwrap();
        let mut nonce = [u8::default(); RPC_NONCE_LEN];
        client.read_exact(&mut nonce).await.unwrap();
        let correct_mac = compute_hmac("correct-horse", &nonce);
        client.write_all(&correct_mac).await.unwrap();
        let mut ack = [0u8; 1];
        client.read_exact(&mut ack).await.unwrap();
        assert_eq!(
            ack[0], 1,
            "the correct secret must be acknowledged as a match"
        );

        let mut client2 = TcpStream::connect(auth_addr).await.unwrap();
        let mut nonce2 = [u8::default(); RPC_NONCE_LEN];
        client2.read_exact(&mut nonce2).await.unwrap();
        let wrong_mac = compute_hmac("totally-wrong", &nonce2);
        client2.write_all(&wrong_mac).await.unwrap();
        let mut ack2 = [0u8; 1];
        client2.read_exact(&mut ack2).await.unwrap();
        assert_eq!(
            ack2[0], 0,
            "a wrong secret must not be acknowledged as a match"
        );
    }

    #[tokio::test]
    async fn admit_via_secret_applies_the_auth_port_offset_and_admits_on_match() {
        // Bind the *real* auth port relative to an arbitrary rpc_port so
        // `admit_via_secret`'s own offset math is exercised end-to-end, not
        // bypassed like the previous test.
        let rpc_port: u16 = 40000 + (std::process::id() % 5000) as u16;
        let auth_port = rpc_port.saturating_add(RPC_AUTH_PORT_OFFSET);
        let bind_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), auth_port);
        let listener = match TcpListener::bind(bind_addr).await {
            Ok(l) => l,
            Err(_) => return, // port already in use on this host; not this test's concern
        };
        tokio::spawn(serve_rpc_auth_port(listener, "cluster-secret".to_string()));

        let ok =
            admit_via_secret(IpAddr::V4(Ipv4Addr::LOCALHOST), rpc_port, "cluster-secret").await;
        assert!(
            ok,
            "matching secret via the real offset-derived port must succeed"
        );
        assert!(is_admitted(&IpAddr::V4(Ipv4Addr::LOCALHOST)));
    }

    #[test]
    fn test_rpc_supervisor_state_tracking() {
        let mut sup = RpcSupervisor::default();
        assert!(!sup.is_healthy(), "default supervisor is not healthy");
        let info = sup.get_info();
        assert_eq!(info.pid, None);
        assert_eq!(info.restart_count, 0);
        assert_eq!(info.consecutive_failures, 0);
        assert!(!info.is_healthy);
    }

    #[test]
    fn test_rpc_supervisor_spawn_and_health_check() {
        let mut sup = RpcSupervisor::default();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        sup.spawn_host = "127.0.0.1".to_string();
        sup.spawn_port = port;

        let child = std::process::Command::new(if cfg!(windows) { "timeout" } else { "sleep" })
            .arg("5")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();

        sup.pid = Some(child.id());
        sup.child = Some(child);
        sup.start_time = Some(std::time::SystemTime::now());
        sup.restart_count = 1;

        assert!(
            sup.check_child_status(),
            "child should be healthy while process is running and port is listening"
        );
        let info = sup.get_info();
        assert!(info.is_healthy);
        assert_eq!(info.restart_count, 1);
        assert_eq!(info.pid, sup.pid);

        sup.stop();
        assert!(
            !sup.is_healthy(),
            "supervisor should not be healthy after stop"
        );
        let info_stopped = sup.get_info();
        assert_eq!(info_stopped.pid, None);
    }

    #[test]
    fn test_evaluate_peer_marks_local_unhealthy_when_child_dead() {
        stop_contributing();
        let node =
            ghostlink_core::protocol::NodeResources::new("local-test", 16.0, 32.0, "8.6", None)
                .with_rpc_port(50052);
        let eval = evaluate_peer(&node, None, None, false, &[], true, false);
        assert!(
            !eval.contribute_compute,
            "effective contribute_compute must be false when child process is down"
        );
        assert_eq!(
            eval.rpc_port, None,
            "rpc_port must be suppressed when child is down"
        );
        assert_eq!(
            eval.excluded_reason,
            Some("rpc child not running".to_string())
        );
    }
    #[test]
    fn validate_distributed_offload_rejects_ngl_zero() {
        let split = vec![0.8, 0.2];
        let res = validate_distributed_offload(0, &split);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("-ngl is 0"));
    }

    #[test]
    fn validate_distributed_offload_rejects_tiny_remote_share() {
        let split = vec![999.0, 1.0]; // 1 / 1000 = 0.1% remote share (< 1% threshold)
        let res = validate_distributed_offload(30, &split);
        assert!(res.is_err());
        assert!(res
            .unwrap_err()
            .contains("below the 1.0% minimum threshold"));
    }

    #[test]
    fn validate_distributed_offload_accepts_valid_remote_share() {
        let split = vec![80.0, 20.0]; // 20% remote share
        let res = validate_distributed_offload(30, &split);
        assert!(res.is_ok());
    }
}
