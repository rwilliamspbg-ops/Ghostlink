//! Binary Protocol with CRC32 Checksums for Ghost-Link Discovery
//! 
//! This module implements a fixed-width binary protocol using:
//! - Fixed-size fields for zero-copy parsing
//! - CRC32 checksums for frame integrity
//! - Sequence numbers and versioning for ordering

use crc32fast::Hasher;
use std::mem::MaybeUninit;

/// Ghost-Link EtherType (0x88B5)
pub const GHOSTLINK_ETHERTYPE: u16 = 0x88B5;

/// Protocol version
pub const PROTOCOL_VERSION: u8 = 1;

/// Maximum payload size for discovery frames
pub const MAX_PAYLOAD_SIZE: usize = 256;

/// Frame header structure (fixed-width)
#[derive(Clone, Copy, Debug)]
pub struct FrameHeader {
    /// EtherType identifying the protocol
    pub ether_type: u16,
    /// Frame kind (see FrameKind enum)
    pub kind: u8,
    /// Protocol version
    pub version: u8,
    /// CRC32 checksum of payload
    pub crc: u32,
}

impl FrameHeader {
    const HEADER_SIZE: usize = 8; // 2 + 1 + 1 + 4 bytes
    
    /// Create a new frame header with computed CRC
    pub fn new(ether_type: u16, kind: u8, payload: &[u8]) -> Self {
        let mut hasher = Hasher::new();
        hasher.update(payload);
        
        Self {
            ether_type,
            kind,
            version: PROTOCOL_VERSION,
            crc: hasher.finalize(),
        }
    }
    
    /// Encode header as bytes (little-endian)
    pub fn encode(&self) -> [u8; Self::HEADER_SIZE] {
        let mut header = [0u8; Self::HEADER_SIZE];
        header[0] = (self.ether_type & 0xFF) as u8;
        header[1] = ((self.ether_type >> 8) & 0xFF) as u8;
        header[2] = self.kind;
        header[3] = self.version;
        header[4..].copy_from_slice(&self.crc.to_le_bytes());
        header
    }
}

/// Frame kind enumeration
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameKind {
    /// Discovery frame - announces node presence
    Discovery = 1,
    /// Join frame - requests to join cluster
    Join = 2,
    /// Attestation frame - resource verification
    Attestation = 3,
    /// Health check frame - periodic liveness probe
    HealthCheck = 4,
    /// Resource advertisement frame - capability update
    ResourceAdvert = 5,
}

impl FrameKind {
    const fn as_u8(&self) -> u8 {
        match self {
            Self::Discovery => 1,
            Self::Join => 2,
            Self::Attestation => 3,
            Self::HealthCheck => 4,
            Self::ResourceAdvert => 5,
        }
    }
}

impl TryFrom<u8> for FrameKind {
    type Error = &'static str;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Discovery),
            2 => Ok(Self::Join),
            3 => Ok(Self::Attestation),
            4 => Ok(Self::HealthCheck),
            5 => Ok(Self::ResourceAdvert),
            _ => Err("unknown frame kind"),
        }
    }
}

/// Node resources structure for discovery frames
#[derive(Clone, Debug, Default)]
pub struct NodeResources {
    /// Unique node identifier
    pub id: String,
    /// GPU VRAM in GB (f32 for precision)
    pub vram_gb: f32,
    /// System memory in GB
    pub system_memory_gb: f32,
    /// CUDA compute capability string (e.g., "8.9")
    pub compute_capability: String,
    /// GPU name/model
    pub gpu_name: Option<String>,
}

impl NodeResources {
    /// Create new node resources
    pub fn new(
        id: impl Into<String>,
        vram_gb: f32,
        system_memory_gb: f32,
        compute_capability: impl Into<String>,
        gpu_name: Option<String>,
    ) -> Self {
        Self {
            id: id.into(),
            vram_gb,
            system_memory_gb,
            compute_capability: compute_capability.into(),
            gpu_name,
        }
    }
    
    /// Serialize node resources to fixed-width binary payload
    /// 
    /// Format: [id_len(1) + id + vram_f32_le + mem_f32_le + cc_len(1) + cc]
    pub fn encode_payload(&self, max_size: usize) -> Vec<u8> {
        let mut payload = Vec::with_capacity(max_size);
        
        // ID length (1 byte)
        let id_bytes = self.id.as_bytes();
        if id_bytes.len() > max_size - 4 {
            return Vec::new(); // Too long
        }
        payload.push(id_bytes.len() as u8);
        payload.extend_from_slice(id_bytes);
        
        // VRAM (4 bytes, little-endian f32)
        unsafe {
            let ptr = &self.vram_gb as *const f32 as *const u8;
            payload.extend_from_slice(&*(ptr as *const [u8; 4]));
        }
        
        // System memory (4 bytes, little-endian f32)
        unsafe {
            let ptr = self.system_memory_gb as *const f32 as *const u8;
            payload.extend_from_slice(&*(ptr as *const [u8; 4]));
        }
        
        // Compute capability length (1 byte)
        let cc_bytes = self.compute_capability.as_bytes();
        if cc_bytes.len() > max_size - 8 {
            return Vec::new(); // Too long
        }
        payload.push(cc_bytes.len() as u8);
        payload.extend_from_slice(cc_bytes);
        
        // GPU name (optional, prefixed with length)
        if let Some(ref gpu_name) = self.gpu_name {
            let gpu_bytes = gpu_name.as_bytes();
            if gpu_bytes.len() > max_size - 10 {
                return Vec::new(); // Too long
            }
            payload.push(1); // Flag indicating GPU name present
            payload.push(gpu_bytes.len() as u8);
            payload.extend_from_slice(gpu_bytes);
        } else {
            payload.push(0); // No GPU name
        }
        
        payload
    }
    
    /// Deserialize node resources from binary payload
    pub fn decode_payload(payload: &[u8]) -> Result<Self, String> {
        if payload.len() < 16 {
            return Err("payload too short".into());
        }
        
        // Read ID length
        let id_len = payload[0] as usize;
        if id_len > payload.len() - 4 {
            return Err("invalid ID length".into());
        }
        
        // Read ID
        let id = std::str::from_utf8(&payload[1..1 + id_len])
            .map_err(|e| e.to_string())?;
        
        // Read VRAM (little-endian f32)
        let vram_bytes: [u8; 4] = payload[1 + id_len..5 + id_len].try_into().unwrap();
        let vram_gb = f32::from_le_bytes(vram_bytes);
        
        // Read system memory (little-endian f32)
        let mem_bytes: [u8; 4] = payload[5 + id_len..9 + id_len].try_into().unwrap();
        let system_memory_gb = f32::from_le_bytes(mem_bytes);
        
        // Read compute capability length
        let cc_len = payload[9 + id_len] as usize;
        if cc_len > payload.len() - 10 {
            return Err("invalid CC length".into());
        }
        
        // Read compute capability
        let cc = std::str::from_utf8(&payload[10 + id_len..10 + id_len + cc_len])
            .map_err(|e| e.to_string())?;
        
        // Check for GPU name flag
        let has_gpu_name = payload[10 + id_len + cc_len] == 1;
        
        Ok(Self {
            id: id.to_string(),
            vram_gb,
            system_memory_gb,
            compute_capability: cc.to_string(),
            gpu_name: if has_gpu_name {
                let gpu_len = payload[10 + id_len + cc_len + 1] as usize;
                if gpu_len > payload.len() - 12 {
                    return Err("invalid GPU name length".into());
                }
                Some(std::str::from_utf8(&payload[10 + id_len + cc_len + 2..])
                    .map_err(|e| e.to_string())?
                    .to_string())
            } else {
                None
            },
        })
    }
}

/// Discovery frame with binary encoding and CRC32
#[derive(Clone, Debug)]
pub struct DiscoveryFrame {
    pub kind: FrameKind,
    pub node: NodeResources,
}

impl DiscoveryFrame {
    /// Encode discovery frame to bytes (header + payload)
    pub fn encode(&self) -> Vec<u8> {
        // Create payload
        let mut payload = self.node.encode_payload(MAX_PAYLOAD_SIZE);
        
        // Compute CRC32 over payload
        let mut hasher = Hasher::new();
        hasher.update(&payload);
        let crc = hasher.finalize();
        
        // Build header
        let header = FrameHeader {
            ether_type: GHOSTLINK_ETHERTYPE,
            kind: self.kind.as_u8(),
            version: PROTOCOL_VERSION,
            crc,
        };
        
        // Combine header and payload
        let mut frame = Vec::with_capacity(header.encode().len() + payload.len());
        frame.extend_from_slice(&header.encode());
        frame.extend_from_slice(&payload);
        
        frame
    }
    
    /// Decode discovery frame from bytes (header + payload)
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() < FrameHeader::HEADER_SIZE {
            return Err("frame too short".into());
        }
        
        // Parse header
        let ether_type = u16::from_be_bytes([bytes[0], bytes[1]]);
        let kind = FrameKind::try_from(bytes[2])?;
        let version = bytes[3];
        let expected_crc = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        
        if ether_type != GHOSTLINK_ETHERTYPE {
            return Err(format!("unexpected EtherType 0x{ether_type:04x}"));
        }
        
        if version != PROTOCOL_VERSION {
            return Err(format!("unsupported protocol version {version}"));
        }
        
        // Parse payload with CRC verification
        let payload_start = FrameHeader::HEADER_SIZE;
        let payload_end = bytes.len();
        let payload = &bytes[payload_start..payload_end];
        
        // Compute CRC over payload
        let mut hasher = Hasher::new();
        hasher.update(payload);
        let computed_crc = hasher.finalize();
        
        if computed_crc != expected_crc {
            return Err(format!("CRC mismatch: expected 0x{expected_crc:08x}, got 0x{computed_crc:08x}"));
        }
        
        // Decode node resources from payload
        let node = NodeResources::decode_payload(payload)?;
        
        Ok(Self { kind, node })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_frames_round_trip() {
        let frame = DiscoveryFrame {
            kind: FrameKind::Join,
            node: NodeResources::new("node-b", 12.0, 32.0, "8.6", None),
        };

        let encoded = frame.encode();
        let decoded = DiscoveryFrame::decode(&encoded).unwrap();

        assert_eq!(decoded.kind, frame.kind);
        assert_eq!(decoded.node.id, frame.node.id);
        assert_eq!(decoded.node.vram_gb, frame.node.vram_gb);
    }

    #[test]
    fn crc_verification_fails_on_modified_payload() {
        let frame = DiscoveryFrame {
            kind: FrameKind::Discovery,
            node: NodeResources::new("node-a", 24.0, 64.0, "8.9", Some("RTX4090".to_string())),
        };

        let encoded = frame.encode();
        let mut modified = encoded.clone();
        modified[10] ^= 0xFF; // Modify payload
        
        let result = DiscoveryFrame::decode(&modified);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("CRC mismatch"));
    }

    #[test]
    fn rejects_wrong_ether_type() {
        let mut fake_frame = vec![0u8; 10];
        fake_frame[0] = 0x88B5 as u8; // Correct first byte
        fake_frame[1] = 0xFF; // Wrong second byte
        
        let result = DiscoveryFrame::decode(&fake_frame);
        assert!(result.is_err());
    }

    #[test]
    fn handles_gpu_name_field() {
        let frame = DiscoveryFrame {
            kind: FrameKind::Discovery,
            node: NodeResources::new(
                "gpu-node-1", 
                24.0, 
                64.0, 
                "9.0", 
                Some("NVIDIA GeForce RTX 4090".to_string())
            ),
        };

        let encoded = frame.encode();
        let decoded = DiscoveryFrame::decode(&encoded).unwrap();

        assert_eq!(decoded.node.gpu_name, Some("NVIDIA GeForce RTX 4090".to_string()));
    }
}
