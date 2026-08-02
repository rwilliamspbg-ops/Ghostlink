//! mDNS-based peer discovery — an alternative path to the UDP broadcast
//! fallback in `discovery.rs` for networks where broadcast traffic is
//! filtered but multicast is not (some managed VLANs, cloud VPCs). Runs
//! alongside UDP discovery rather than replacing it: callers typically merge
//! results from both.
//!
//! A Ghostlink node advertises itself as an instance of `_ghostlink._tcp.local.`
//! with its `NodeResources` encoded as TXT record key/value pairs. Peers
//! browse for that service type and decode the TXT records back into
//! `NodeResources`.

use std::net::{IpAddr, SocketAddr};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use mdns_sd::{ResolvedService, ServiceDaemon, ServiceEvent, ServiceInfo};

use crate::protocol::NodeResources;

/// mDNS service type Ghostlink nodes advertise themselves under.
pub const MDNS_SERVICE_TYPE: &str = "_ghostlink._tcp.local.";

static ADVERTISER: OnceLock<Option<ServiceDaemon>> = OnceLock::new();

/// Registers this node under [`MDNS_SERVICE_TYPE`] so LAN peers can find it
/// via mDNS browsing, idempotently (subsequent calls are no-ops). Best-effort:
/// a failure (e.g. multicast unavailable in a sandboxed/headless environment)
/// is logged and otherwise ignored — mirrors how UDP discovery failures never
/// block server startup elsewhere in this crate.
pub fn ensure_advertised(node: &NodeResources, port: u16) {
    ADVERTISER.get_or_init(|| match advertise(node, port) {
        Ok(daemon) => Some(daemon),
        Err(err) => {
            tracing::warn!("mdns: failed to advertise local node, continuing without it: {err}");
            None
        }
    });
}

fn advertise(node: &NodeResources, port: u16) -> Result<ServiceDaemon, String> {
    let daemon = ServiceDaemon::new().map_err(|e| format!("failed to start mDNS daemon: {e}"))?;

    let properties = node_to_txt_properties(node);
    let borrowed_properties: Vec<(&str, &str)> = properties
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();

    let instance_name = sanitize_instance_name(&node.id);
    let host_name = format!("{instance_name}.local.");

    // Empty ip + enable_addr_auto() lets the daemon discover and advertise
    // this host's real interface addresses instead of us guessing one.
    let service = ServiceInfo::new(
        MDNS_SERVICE_TYPE,
        &instance_name,
        &host_name,
        "",
        port,
        borrowed_properties.as_slice(),
    )
    .map_err(|e| format!("failed to build mDNS service info: {e}"))?
    .enable_addr_auto();

    daemon
        .register(service)
        .map_err(|e| format!("failed to register mDNS service: {e}"))?;

    Ok(daemon)
}

/// Browses for other Ghostlink nodes over mDNS for up to `timeout`, returning
/// each resolved peer's advertised resources and the address to reach it on.
/// Blocks the calling thread for up to `timeout` — callers on an async
/// runtime should run this via `spawn_blocking`, same as
/// `discovery::broadcast_and_collect`.
pub fn discover(timeout: Duration) -> Result<Vec<(NodeResources, SocketAddr)>, String> {
    let daemon = ServiceDaemon::new().map_err(|e| format!("failed to start mDNS daemon: {e}"))?;
    let receiver = daemon
        .browse(MDNS_SERVICE_TYPE)
        .map_err(|e| format!("failed to browse {MDNS_SERVICE_TYPE}: {e}"))?;

    // Keyed by node id so a peer re-announcing during the browse window
    // (mDNS resends resolved records periodically) doesn't produce
    // duplicate entries.
    let mut peers = std::collections::HashMap::new();
    let deadline = Instant::now() + timeout;

    while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
        match receiver.recv_timeout(remaining) {
            Ok(ServiceEvent::ServiceResolved(info)) => {
                if let (Some(node), Some(addr)) =
                    (node_from_service_info(&info), first_socket_addr(&info))
                {
                    peers.insert(node.id.clone(), (node, addr));
                }
            }
            Ok(_) => {}
            Err(_) => break, // timed out or the channel closed
        }
    }

    let _ = daemon.stop_browse(MDNS_SERVICE_TYPE);
    let _ = daemon.shutdown();

    Ok(peers.into_values().collect())
}

fn first_socket_addr(info: &ResolvedService) -> Option<SocketAddr> {
    info.get_addresses_v4()
        .into_iter()
        .next()
        .map(|ip| SocketAddr::new(IpAddr::V4(ip), info.get_port()))
}

fn node_from_service_info(info: &ResolvedService) -> Option<NodeResources> {
    node_from_txt_lookup(|key| info.get_property_val_str(key).map(str::to_owned))
}

fn node_to_txt_properties(node: &NodeResources) -> Vec<(String, String)> {
    let mut props = vec![
        ("id".to_string(), node.id.clone()),
        ("vram_gb".to_string(), node.vram_gb.to_string()),
        (
            "system_memory_gb".to_string(),
            node.system_memory_gb.to_string(),
        ),
        (
            "compute_capability".to_string(),
            node.compute_capability.clone(),
        ),
    ];
    if let Some(gpu_name) = &node.gpu_name {
        props.push(("gpu_name".to_string(), gpu_name.clone()));
    }
    if let Some(rpc_port) = node.rpc_port {
        props.push(("rpc_port".to_string(), rpc_port.to_string()));
    }
    props
}

fn node_from_txt_lookup(lookup: impl Fn(&str) -> Option<String>) -> Option<NodeResources> {
    let id = lookup("id")?;
    let vram_gb = lookup("vram_gb")?.parse().ok()?;
    let system_memory_gb = lookup("system_memory_gb")?.parse().ok()?;
    let compute_capability = lookup("compute_capability").unwrap_or_default();
    let gpu_name = lookup("gpu_name");
    let rpc_port = lookup("rpc_port").and_then(|v| v.parse().ok());
    Some(NodeResources {
        id,
        vram_gb,
        system_memory_gb,
        compute_capability,
        gpu_name,
        rpc_port,
    })
}

/// mDNS instance names conventionally avoid raw dots (the DNS label
/// separator) — collapse them so an id containing one doesn't produce a
/// confusing on-wire label.
fn sanitize_instance_name(id: &str) -> String {
    id.replace('.', "-")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lookup_from(props: &[(String, String)]) -> impl Fn(&str) -> Option<String> + '_ {
        move |key: &str| props.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone())
    }

    #[test]
    fn txt_roundtrip_preserves_node_fields() {
        let node = NodeResources::new("node-a", 24.0, 64.0, "8.9", Some("RTX 4090".to_string()));
        let props = node_to_txt_properties(&node);
        let decoded = node_from_txt_lookup(lookup_from(&props)).expect("decode");

        assert_eq!(decoded.id, node.id);
        assert_eq!(decoded.vram_gb, node.vram_gb);
        assert_eq!(decoded.system_memory_gb, node.system_memory_gb);
        assert_eq!(decoded.compute_capability, node.compute_capability);
        assert_eq!(decoded.gpu_name, node.gpu_name);
    }

    #[test]
    fn txt_roundtrip_without_gpu_name() {
        let node = NodeResources::new("node-b", 8.0, 16.0, "7.5", None);
        let props = node_to_txt_properties(&node);
        let decoded = node_from_txt_lookup(lookup_from(&props)).expect("decode");

        assert_eq!(decoded.gpu_name, None);
    }

    #[test]
    fn txt_roundtrip_preserves_rpc_port() {
        let node = NodeResources::new("node-c", 12.0, 32.0, "8.6", None).with_rpc_port(50052);
        let props = node_to_txt_properties(&node);
        let decoded = node_from_txt_lookup(lookup_from(&props)).expect("decode");

        assert_eq!(decoded.rpc_port, Some(50052));
    }

    #[test]
    fn txt_roundtrip_without_rpc_port() {
        let node = NodeResources::new("node-d", 8.0, 16.0, "7.5", None);
        let props = node_to_txt_properties(&node);
        let decoded = node_from_txt_lookup(lookup_from(&props)).expect("decode");

        assert_eq!(decoded.rpc_port, None);
    }

    #[test]
    fn decode_fails_gracefully_on_missing_required_field() {
        // No "id" key present.
        let props = vec![("vram_gb".to_string(), "8.0".to_string())];
        assert!(node_from_txt_lookup(lookup_from(&props)).is_none());
    }

    #[test]
    fn sanitize_instance_name_collapses_dots() {
        assert_eq!(
            sanitize_instance_name("workstation.local"),
            "workstation-local"
        );
        assert_eq!(sanitize_instance_name("node-a"), "node-a");
    }
}
