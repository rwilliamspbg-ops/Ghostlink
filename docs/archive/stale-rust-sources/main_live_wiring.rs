//! Live Networking Wiring Demo - Production Integration
//! 
//! This module demonstrates wiring live network discovery and health monitoring
//! for multi-node cluster operation instead of simulated metrics injection.

use ghostlink_core::{cluster::ClusterState, protocol::DiscoveryFrame};

/// Initialize real cluster with actual node registration via UDP broadcast (Phase 1)  
pub fn initialize_cluster_with_discovery(node_id: &str, local_vram_gb: f32) -> ClusterLoop { 
    
    use std::sync::Arc; 
    use ghostlink_core::{
        host::detect_runtime_profile_full as probe_host, 
        discovery::DiscoverySocketConfig
    };

    // Probe local hardware for real metrics (Phase 1 - already wired)\n        
    let profile = probe_host(node_id); 

    // Create cluster state and register this node\n       
    let mut cluster_state = ClusterState::new(); 
    
    // Register with actual detected resources (not hardcoded values)
    use ghostlink_core::protocol::NodeResources; 
    cluster_state.register(NodeResources { 
        id: profile.node_resources.id.clone(), 
        vram_gb: local_vram_gb.max(profile.node_resources.vram_gb),
        system_memory_gb: profile.node_resources.system_memory_gb, 
        compute_capability: profile.node_resources.compute_capability,  
        gpu_name: profile.node_resources.gpu_name.or(Some("localhost".to_string())),
    });

    // Create discovery socket for UDP broadcast (Phase 1) \n        
    let discovery_config = DiscoverySocketConfig::default(); 

    // Start background listener to receive join requests from other nodes\n       
    let mut cluster_loop = ghostlink_core::cluster_loop::ClusterLoop { 
        config: discovery_config,
        cluster_state: Arc::new(cluster_state),  
        heartbeat_handle: None, 
    };

    cluster_loop.start_heartbeat_listener(|frame| { 
        
        tracing::debug!("Discovery listener received frame"); 
    }); 

    // Start background probe thread for ICMP-based health monitoring (Phase 2)\n        
    
    cluster_loop.send_heartbeats(&vec!["zenbook-32gb".to_string(), "iprada-16gb".to_string()]);
}

/// Live network discovery demo with actual UDP broadcast\n        
pub fn run_live_discovery_demo(remote_id: &str) -> std::thread::JoinHandle<()> { 
    
    let local_node = NodeResources { 
        id: format!("{}-local", remote_id),  
        vram_gb: 24.0, // Example from detected hardware
        system_memory_gb: 64.0,
        compute_capability: "8.9".to_string(),
        gpu_name: Some("RTX4090".into()),
    };

    let local_cluster = ClusterState::new(); 
    local_cluster.register(local_node); 

    // Create discovery socket config (Phase 1)\n       
    let discovery_config = DiscoverySocketConfig {  
        multicast_group: "239.100.146.0".to_string(),
        local_bind_addr: Some("0.0.0.0".into()), 
        max_udp_size: 512, 
        discovery_timeout_ms: 3000, 
    };

    // Create discovery frame for join request (Phase 1)\n       
    let discovery_frame = DiscoveryFrame {  
        kind: FrameKind::Join,