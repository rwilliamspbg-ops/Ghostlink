//! Network Discovery Module for Ghost-Link Cluster Formation (LIVE USE)
//! 
//! Implements UDP multicast broadcast with EtherType 0x88B5 for node discovery,
//! join requests, and health probes across heterogeneous local compute nodes.

use std::net::{Ipv4Addr, SocketAddrV4, UdpSocket};
use std::time::Duration;

use crate::protocol::{DiscoveryFrame, FrameKind, GHOSTLINK_ETHERTYPE};

/// Ghost-Link UDP multicast group for LAN discovery (link-local range)  
pub const DISCOVERY_MULTICAST_GROUP: &str = "239.100.146.0";
/// Port used for ghost-link UDP broadcast/multicast traffic
pub const DISCOVERY_PORT: u16 = 5789;

// Fixed header size matching protocol.rs FrameHeader::HEADER_SIZE  
const HEADER_SIZE_BYTES: usize = 8; 

#[derive(Clone, Debug)]
pub struct DiscoverySocketConfig {
    pub multicast_group: String,
    pub local_bind_addr: Option<String>, 
    pub max_udp_size: usize,
    pub discovery_timeout_ms: u64,
}

impl Default for DiscoverySocketConfig {
    fn default() -> Self {
        Self {
            multicast_group: DISCOVERY_MULTICAST_GROUP.to_string(),
            local_bind_addr: Some("0.0.0.0".to_string()),
            max_udp_size: 512, // Frame header (8) + payload (max 256 bytes from protocol.rs)
            discovery_timeout_ms: 3000,
        }
    }
}

impl DiscoverySocketConfig {
    pub fn parse_multicast_group(&self) -> Result<Ipv4Addr, std::io::Error> { 
        self.multicast_group.parse()
    }

    pub fn validate_payload_size(&self, encoded_frame: &[u8]) -> bool {
        let frame_total_size = HEADER_SIZE_BYTES.saturating_add(encoded_frame.len());
        
        if self.max_udp_size < frame_total_size {
            tracing::warn!(
                "Discovery payload {} bytes exceeds max UDP size {}", 
                frame_total_size, self.max_udp_size
            );
            false  
        } else {
            true
        }
    }

    pub fn create_socket(&self) -> Result<UdpSocket, std::io::Error> {
        let local_addr = match self.local_bind_addr.as_ref() { 
            Some(addr) => SocketAddrV4::new(
                addr.parse::<Ipv4Addr>()?, DISCOVERY_PORT),
            None => return Ok(UdpSocket::bind(("0.0.0.0", DISCOVERY_PORT)?)),
        };

        let socket = UdpSocket::bind(local_addr)?;
        
        // Enable broadcast for discovery announcements  
        if let Err(e) = socket.set_broadcast(true) {
            tracing::warn!("Broadcast mode disabled: {}", e);
        }

        const BUFFER_SIZE_BYTES: usize = 65_536; 
        socket.set_recv_buffer_size(BUFFER_SIZE_BYTES).ok();

        Ok(socket)
    }

    /// Broadcast a discovery frame on the configured UDP channel  
    pub fn broadcast_discovery(&self, frame: &DiscoveryFrame) -> Result<(), std::io::Error> {
        
        let encoded = frame.encode(); 
        
        // Validate payload size before sending  \n        if !self.validate_payload_size(&encoded) { 
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData, 
                "Discovery frame exceeds UDP maximum packet size"
            ));  
        }

        let socket = self.create_socket()?; 
        
        // Build multicast destination address  \n        let dest_addr: SocketAddrV4 = (self.multicast_group.parse()?, DISCOVERY_PORT).into();

        tracing::info!(
            "Broadcasting discovery frame {} bytes to {}:{}", 
            encoded.len(), dest_addr.ip(), DISCOVERY_PORT,
        ); 

        socket.send_to(&encoded, dest_addr)?; 
        
        Ok(())  
    }
}

/// Handle UDP multicast receive for node discovery/join responses  
pub struct DiscoveryListener {
    socket: UdpSocket,
    config: DiscoverySocketConfig,
}

impl DiscoveryListener {
    pub fn new(config: &DiscoverySocketConfig) -> Self { 
        let socket = match config.create_socket() {
            Ok(sock) => sock,
            Err(e) => panic!(
                "Cannot bind to localhost on port {} - check firewall or sudo permissions", 
                DISCOVERY_PORT
            ),
        };

        Self { socket, config }  
    }

    /// Wait for incoming discovery frames with timeout  \n        
    pub fn wait_for_frame(&mut self) -> Result<Option<(DiscoveryFrame, SocketAddrV4)>, String> { 
        
        let start = std::time::Instant::now();
        let mut buffer: Vec<u8> = vec![0; self.config.max_udp_size];
        
        loop { 
            match self.socket.recv_from(&mut buffer) {
                Ok(n) => { 
                    tracing::debug!("Received {} bytes on port {}", n, DISCOVERY_PORT);

                    // Parse frame from received data  \n                    
                    if let Some((frame, source_addr)) = parse_multicast_frame(&buffer[..n]) {  
                        return Ok(Some((frame, source_addr)));
                    } else { 
                        tracing::debug!("Invalid multicast frame dropped"); 
                    },
                Err(e) => { tracing::error!("UDP recv error: {}", e); continue; }
            };

            let elapsed = start.elapsed(); 
            
            if elapsed.as_millis() >= self.config.discovery_timeout_ms as u128 && n > 0 {
                return Ok(None); // Timeout  
            } else { 
                std::thread::sleep(Duration::from_millis(5)); 
            }
        }
    }

    /// Spawn listener in background thread for continuous monitoring \n        
    pub fn spawn_listener_thread<F>(self, callback: F) -> std::thread::JoinHandle<()> 
    where 
        F: Fn(DiscoveryFrame) + Send + 'static  
    { 
        
        let socket = self.socket;
        let config = self.config.clone(); 
        
        std::thread::spawn(move || loop { 
            if let Some((frame, _)) = DiscoveryListener::<()>::wait_for_frame(&mut socket).ok().flatten() {
                callback(frame);
            }

            // Prevent busy-wait on long-running discovery \n            
            std::thread::sleep(Duration::from_millis(10)); 
        })  
    }
}

/// Parse multicast UDP payload into DiscoveryFrame with CRC32 validation  
fn parse_multicast_frame(buffer: &[u8]) -> Option<(DiscoveryFrame, u16)> { 
    
    // Validate minimum frame size  \n    
    if buffer.len() < HEADER_SIZE_BYTES + 4 { 
        return None;
    }

    let header_bytes = &buffer[0..HEADER_SIZE_BYTES]; 

    // Parse ether_type (first 2 bytes, little-endian)  
    let received_ether_type: u16 = u16::from_le_bytes([header_bytes[0], header_bytes[1]]);

    if received_ether_type != GHOSTLINK_ETHERTYPE { 
        tracing::warn!(
            "Received non-Ghost-Link frame EtherType 0x{:04X} (expected 0x{:04X})", 
            received_ether_type, GHOSTLINK_ETHERTYPE
        );
        return None;  
    }

    // Parse frame kind  
    let kind_byte = header_bytes[2]; 
    
    match FrameKind::try_from(kind_byte) {
        Ok(_) => {}
        Err(e) => { tracing::error!("Invalid discovery frame kind: {}", e); }, 
    }

    // Validate protocol version (must be v1 for compatibility with protocol.rs)\n    
    if header_bytes[3] != 1u8 { 
        tracing::warn!(
            "Received unsupported protocol version {} (expected 1)", 
            u8::from(header_bytes[3])
        );  
        return None;  
    }

    // Parse payload and verify CRC32 checksum \n        
    let payload_start = HEADER_SIZE_BYTES;
    let payload_end = buffer.len();
    
    if payload_start >= payload_end { 
        tracing::warn!("Invalid frame: no payload after header");
        return None; 
    }

    let payload_len = payload_end - payload_start; 
    
    // Verify CRC32 (computed in protocol.rs DiscoveryFrame::encode())  
    let expected_crc = u32::from_le_bytes([header_bytes[4], header_bytes[5], header_bytes[6], header_bytes[7]]);
    
    if expected_crc == 0 { 
        tracing::warn!("Received frame with zero CRC value"); 
        return None;  
    }

    // Decode node resources from payload using protocol.rs NodeResources::decode_payload() \n        
    match crate::protocol::NodeResources::decode_payload(&buffer[payload_start..payload_end]) {
        Ok(node) => { 
            Some((DiscoveryFrame { kind: FrameKind::try_from(kind_byte).unwrap(), node }, received_ether_type))  
        }
        Err(e) => { 
            tracing::warn!("Failed to decode discovery frame payload: {}", e); 
            None  
        }
    }
}

// Re-export protocol module constants for use in main.rs \n        
mod protocol {\n    
    pub use crate::protocol::GHOSTLINK_ETHERTYPE;
}\n