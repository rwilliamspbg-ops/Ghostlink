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
//! reach the port can submit compute to it. Only enable contribution
//! (`contribute_compute` in settings) on a network you trust, same
//! assumption the existing UDP/mDNS discovery already makes.

use std::net::SocketAddr;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock};

use ghostlink_core::cluster::{ClusterState, NodeStatus};

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
pub fn ensure_contributing(bind_host: &str, port: u16) {
    let handle = contributor_handle();
    let mut guard = handle.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(child) = guard.as_mut() {
        if matches!(child.try_wait(), Ok(None)) {
            return; // already running
        }
    }

    let bin = get_rpc_server_bin();
    tracing::warn!(
        "rpc_cluster: starting ggml-rpc-server on {bind_host}:{port} \u{2014} this exposes local \
         compute (GPU/CPU) to the network with NO AUTHENTICATION (an upstream llama.cpp \
         limitation, not Ghostlink's). Only enable contribute_compute on a network you trust, \
         the same assumption UDP/mDNS discovery already makes."
    );

    match Command::new(&bin)
        .arg("-H")
        .arg(bind_host)
        .arg("-p")
        .arg(port.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
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

/// A cluster peer known to be contributing compute over `ggml-rpc-server`.
#[derive(Debug, Clone, PartialEq)]
pub struct RpcPeer {
    pub node_id: String,
    pub addr: SocketAddr,
    pub vram_gb: f32,
}

/// Finds healthy, RPC-contributing peers (excluding `local_node_id`) from
/// `cluster`'s live node list. A peer only qualifies if it advertised an
/// `rpc_port` (opted in to contributing), is `NodeStatus::Active`, and has a
/// known reachable IP (from discovery or `/api/workers/connect`). Sorted by
/// node id for a stable `--rpc`/`--tensor-split` ordering across a session.
pub fn discover_rpc_peers(cluster: &ClusterState, local_node_id: &str) -> Vec<RpcPeer> {
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
        let mut node = NodeResources::new(id, vram_gb, 32.0, "8.6", None);
        if let Some(port) = rpc_port {
            node = node.with_rpc_port(port);
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

        let peers = discover_rpc_peers(&cluster, "local");

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

        assert!(discover_rpc_peers(&cluster, "local").is_empty());
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

        let peers = discover_rpc_peers(&cluster, "local");
        assert_eq!(peers.len(), 2);
        assert_eq!(peers[0].node_id, "alpha");
        assert_eq!(peers[1].node_id, "zeta");
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
}
