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

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock};

use tokio::io::copy_bidirectional;
use tokio::net::{TcpListener, TcpStream};
use tokio::runtime::Handle;

use ghostlink_core::cluster::{ClusterState, NodeStatus};

/// Offset added to the publicly-advertised `rpc_port` to derive the
/// loopback-only port `ggml-rpc-server` actually binds to when the
/// allowlist proxy is active. Arbitrary but fixed, so the proxy and
/// `ggml-rpc-server` agree on it across the periodic respawn-supervision
/// calls without needing to thread extra state between them.
/// `saturating_add`ed against the configured port, so a `rpc_port` already
/// close to `u16::MAX` degrades (a possible port reuse) rather than
/// overflowing/panicking.
const RPC_INTERNAL_PORT_OFFSET: u16 = 1000;

/// True once this process has spawned its (single, process-lifetime)
/// allowlist proxy task. Guards `maybe_start_allowlist_proxy` so the
/// periodic respawn-supervision loop in `main.rs` (which calls
/// `ensure_contributing` every 30s) doesn't try to bind the public port a
/// second time.
static RPC_ALLOWLIST_PROXY_STARTED: OnceLock<()> = OnceLock::new();

static RPC_CONTRIBUTOR_PROCESS: OnceLock<Arc<Mutex<Option<Child>>>> = OnceLock::new();

fn contributor_handle() -> Arc<Mutex<Option<Child>>> {
    RPC_CONTRIBUTOR_PROCESS
        .get_or_init(|| Arc::new(Mutex::new(None)))
        .clone()
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
/// When `allowed_peers` is non-empty, `ggml-rpc-server` is bound
/// loopback-only and an allowlist proxy (spawned onto `rt_handle`, since
/// this function itself isn't async and may be called before the tokio
/// runtime's `block_on` starts) is stood up on the publicly-advertised
/// `bind_host:port` in front of it — see this module's top-of-file SECURITY
/// doc. When `allowed_peers` is empty, `ggml-rpc-server` binds
/// `bind_host:port` directly, exactly as before this allowlist existed: no
/// proxy, no extra hop, no behavior change for the default case.
pub fn ensure_contributing(
    bind_host: &str,
    port: u16,
    allowed_peers: &[String],
    rt_handle: &Handle,
) {
    let handle = contributor_handle();
    let mut guard = handle.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(child) = guard.as_mut() {
        if matches!(child.try_wait(), Ok(None)) {
            return; // already running
        }
    }

    let (spawn_host, spawn_port): (String, u16) = if allowed_peers.is_empty() {
        (bind_host.to_string(), port)
    } else {
        let internal_port = port.saturating_add(RPC_INTERNAL_PORT_OFFSET);
        maybe_start_allowlist_proxy(bind_host, port, internal_port, allowed_peers, rt_handle);
        ("127.0.0.1".to_string(), internal_port)
    };

    let bin = get_rpc_server_bin();
    if allowed_peers.is_empty() {
        tracing::warn!(
            "rpc_cluster: starting ggml-rpc-server on {spawn_host}:{spawn_port} \u{2014} this \
             exposes local compute (GPU/CPU) to the network with NO AUTHENTICATION (an upstream \
             llama.cpp limitation, not Ghostlink's) and NO IP ALLOWLIST (rpc_allowed_peers is \
             empty). Only enable contribute_compute on a network you trust, the same assumption \
             UDP/mDNS discovery already makes. Set rpc_allowed_peers in settings to restrict \
             which hosts may submit compute jobs."
        );
    } else {
        tracing::warn!(
            "rpc_cluster: starting ggml-rpc-server on loopback ({spawn_host}:{spawn_port}), \
             fronted by an allowlist proxy on {bind_host}:{port} restricted to {} \
             rpc_allowed_peers entries \u{2014} ggml-rpc-server itself still has NO \
             AUTHENTICATION (an upstream llama.cpp limitation), so this is access control, not \
             protocol-level auth. A device already inside an allowlisted range isn't stopped by \
             this.",
            allowed_peers.len()
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
        Ok(child) => *guard = Some(child),
        Err(err) => {
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
    let handle = contributor_handle();
    if let Ok(mut guard) = handle.lock() {
        if let Some(mut child) = guard.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    };
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

    // Only the thread whose `set()` succeeds spawns — race-free without an
    // extra Mutex, and cheap even under the (practically impossible, given
    // `ensure_contributing` already serializes callers through its own
    // `contributor_handle()` lock) case of concurrent first calls.
    if RPC_ALLOWLIST_PROXY_STARTED.set(()).is_ok() {
        rt_handle.spawn(async move {
            if let Err(err) = run_rpc_allowlist_proxy(public_addr, backend_addr, allowed).await {
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
) -> std::io::Result<()> {
    let listener = TcpListener::bind(public_addr).await?;
    tracing::info!(
        "rpc_cluster: rpc_allowed_peers proxy listening on {public_addr}, forwarding allowed \
         peers to ggml-rpc-server at {backend_addr} ({} allowlist entries)",
        allowed_peers.len()
    );
    serve_rpc_allowlist_proxy(listener, backend_addr, allowed_peers).await
}

/// Accept loop shared by `run_rpc_allowlist_proxy` and its tests: for each
/// inbound connection, checks the peer's source IP against `allowed_peers`
/// (`ip_allowed`) and either splices it through to `backend_addr` via
/// `tokio::io::copy_bidirectional`, or drops it immediately with a
/// `tracing::warn!` naming the rejected IP. Split out from
/// `run_rpc_allowlist_proxy` so tests can bind an ephemeral loopback port
/// themselves (avoiding a bind/rebind race) instead of going through a
/// fixed `SocketAddr`.
async fn serve_rpc_allowlist_proxy(
    listener: TcpListener,
    backend_addr: SocketAddr,
    allowed_peers: Vec<String>,
) -> std::io::Result<()> {
    loop {
        let (inbound, peer_addr) = match listener.accept().await {
            Ok(pair) => pair,
            Err(err) => {
                tracing::warn!("rpc_cluster: rpc_allowed_peers proxy accept() failed: {err}");
                continue;
            }
        };

        if !ip_allowed(&peer_addr.ip(), &allowed_peers) {
            tracing::warn!(
                "rpc_cluster: rejected ggml-rpc-server connection from {} \u{2014} not in \
                 rpc_allowed_peers",
                peer_addr.ip()
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
pub fn discover_rpc_peers(
    cluster: &ClusterState,
    local_node_id: &str,
    local_rpc_build_id: Option<&str>,
) -> Vec<RpcPeer> {
    let nodes = cluster.nodes_snapshot();
    let mut peers: Vec<RpcPeer> = nodes
        .iter()
        .filter(|node| node.id != local_node_id)
        .filter_map(|node| {
            let rpc_port = node.rpc_port?;
            let metrics = cluster.get_metrics(&node.id)?;
            if metrics.status != NodeStatus::Active {
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
            })
        })
        .collect();
    peers.sort_by(|a, b| a.node_id.cmp(&b.node_id));
    peers
}

/// Computes a `--tensor-split` ratio: the local device first, then each
/// remote peer in the same order as `peers` — llama.cpp registers backend
/// devices in exactly that order (local device(s), then `--rpc` targets in
/// the order given), so the split array's positions must line up with it.
/// Proportional to each device's reported VRAM; llama.cpp normalizes the
/// values itself; these don't need to sum to 1.
pub fn compute_tensor_split(local_vram_gb: f32, peers: &[RpcPeer]) -> Vec<f32> {
    let mut split = vec![local_vram_gb.max(0.1)];
    split.extend(peers.iter().map(|p| p.vram_gb.max(0.1)));
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
        register_peer(
            &cluster,
            "contributor",
            12.0,
            Some(50052),
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
        register_peer(
            &cluster,
            "contributor",
            12.0,
            Some(50052),
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
            },
            RpcPeer {
                node_id: "b".to_string(),
                addr: loopback(2),
                vram_gb: 8.0,
            },
        ];
        let split = compute_tensor_split(16.0, &peers);
        assert_eq!(split, vec![16.0, 12.0, 8.0]);
    }

    #[test]
    fn tensor_split_floors_zero_vram_to_avoid_a_dead_device_entry() {
        let split = compute_tensor_split(0.0, &[]);
        assert_eq!(split, vec![0.1]);
    }

    #[test]
    fn rpc_flag_value_formats_host_port_list() {
        let peers = vec![
            RpcPeer {
                node_id: "a".to_string(),
                addr: loopback(50052),
                vram_gb: 8.0,
            },
            RpcPeer {
                node_id: "b".to_string(),
                addr: loopback(50053),
                vram_gb: 8.0,
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
}
