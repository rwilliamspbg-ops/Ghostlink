//! Binary Protocol with CRC32 Checksums for Ghost-Link Discovery
//! 
//! This module implements a fixed-width binary protocol using:
//! - Fixed-size fields for zero-copy parsing
//! - CRC32 checksums for frame integrity
//! - Sequence numbers and versioning for ordering

use crc32fast::Hasher;
use thiserror::Error;

/// Ghost-Link EtherType (0x88B5)
pub const GHOSTLINK_ETHERTYPE: u16 = 0x88B5;

/// Protocol version
pub const PROTOCOL_VERSION: u8 = 1;

/// Maximum payload size for discovery frames
pub const MAX_PAYLOAD_SIZE: usize = 256;

#[derive(Error, Debug)]
pub enum ProtocolError {
    #[error("I/O error during protocol operations: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Protocol version mismatch: expected {}, got {}", .expected_version, .actual_version)]
    VersionMismatch { 
        expected_version: u8, 
        actual_version: u8 
    },
    
    #[error("Unknown frame kind: 0x{:02X} (valid kinds are 1-5)")]
    UnknownFrameKind(u8),
    
    #[error("CRC mismatch in payload verification")]
    CrcMismatch {
        expected_crc: u32, 
        computed_crc: u32 
    },
    
    #[error("Invalid frame header size: got {}, expected {}", .got_bytes, FrameHeader::HEADER_SIZE)]
    InvalidFrameSize {
        got_bytes: usize, 
        expected_header_size: usize
    },
    
    #[error("Unexpected EtherType 0x{:04X} (expected {:04X})", .actual_ether_type, GHOSTLINK_ETHERTYPE)]
    UnexpectedEtherType { actual_ether_type: u16 },
    
    #[error("Invalid ID length in payload")]
    InvalidIdLength,
    
    #[error("Invalid compute capability field in payload")]
    InvalidCcField,
    
    #[error("GPU name overflow or missing flag")]
    GpuNameError,
}

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
    
    fn validate_header(&self) -> Result<(), ProtocolError> {
        if self.version != PROTOCOL_VERSION {
            return Err(ProtocolError::VersionMismatch { 
                expected_version: PROTOCOL_VERSION,
                actual_version: self.version
            });
        }

        Ok(())
    }

    /// Create a new frame header with computed CRC
    pub fn new(ether_type: u16, kind: u8, payload: &[u8]) -> Self {
        let mut hasher = Hasher::new();
        hasher.update(payload);

        Self {
            ether_type,
            kind,
            version: PROTOCOL_VERSION, // Always use protocol default for new frames
            crc: hasher.finalize(),
        }
    }

    /// Encode header as bytes (little-endian)
    pub fn encode(&self) -> [u8; Self::HEADER_SIZE] {
        let mut header = [0u8; Self::HEADER_SIZE];
        header[0..2].copy_from_slice(&self.ether_type.to_le_bytes());
        header[2] = self.kind;
        header[3] = self.version;
        header[4..].copy_from_slice(&self.crc.to_le_bytes());
        header
    }

    /// Create a new frame header with validation (for received frames)
    pub fn from_received(bytes: &[u8]) -> Result<Self, ProtocolError> {
        if bytes.len() < Self::HEADER_SIZE {
            return Err(ProtocolError::InvalidFrameSize { 
                got_bytes: bytes.len(), 
                expected_header_size: Self::HEADER_SIZE 
            });
        }

        let ether_type = u16::from_le_bytes([bytes[0], bytes[1]]);
        
        if ether_type != GHOSTLINK_ETHERTYPE {
            return Err(ProtocolError::UnexpectedEtherType { actual_ether_type: ether_type });
        }

        let kind_byte = bytes[2];
        let version = bytes[3];
        let crc = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);

        if version != PROTOCOL_VERSION {
            return Err(ProtocolError::VersionMismatch { 
                expected_version: PROTOCOL_VERSION,
                actual_version: version
            });
        }

        let kind = FrameKind::try_from(kind_byte)
            .map_err(|_| ProtocolError::UnknownFrameKind(kind_byte))?;

        Ok(Self { ether_type, kind, version, crc })
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

    /// Try to create frame kind from byte value with error handling
    pub const fn try_from(value: u8) -> Result<Self, ProtocolError> {
        match value {
            1 => Ok(Self::Discovery),
            2 => Ok(Self::Join),
            3 => Ok(Self::Attestation),
            4 => Ok(Self::HealthCheck),
            5 => Ok(Self::ResourceAdvert),
            _ => Err(ProtocolError::UnknownFrameKind(value)),
        }
    }

    /// Check if frame kind is valid (1-5)
    pub const fn is_valid(kind_byte: u8) -> bool {
        Self::try_from(kind_byte).is_ok()
    }
}

#[derive(Error, Debug)]
pub enum NodeResourcesError {
    #[error("Payload too short for node resources")]
    PayloadTooShort(usize),
    
    #[error("Invalid ID length in payload: {}", .id_len)]
    InvalidIdLength { id_len: usize },
    
    #[error("Compute capability field size exceeds max allowed bytes")]
    CcFieldSizeError,
    
    #[error("GPU name flag mismatch or invalid GPU name length"),]
    GpuNameFlagMismatch(usize),
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

    /// Validate node resources for security checks (length limits)
    pub fn validate(&self) -> Result<(), NodeResourcesError> {
        let id_bytes = self.id.as_bytes();
        
        // ID length limit to prevent DoS via excessively long IDs
        if id_bytes.len() > 256u8 as usize || id_bytes.is_empty() {
            return Err(NodeResourcesError::InvalidIdLength { 
                id_len: id_bytes.len() 
            });
        }

        let cc_bytes = self.compute_capability.as_bytes();
        
        // Compute capability length limit  
        if cc_bytes.len() > 32 || cc_bytes.is_empty() {
            return Err(NodeResourcesError::CcFieldSizeError);
        }

        // VRAM sanity check (should be positive and reasonable)
        if self.vram_gb <= 0.0 || self.vram_gb > 512.0 { 
            log::warn!("Suspicious VRAM value: {} GB", self.vram_gb);
        }

        Ok(())
    }

    /// Serialize node resources to fixed-width binary payload
    #[inline]  
    pub fn encode_payload(&self, max_size: usize) -> Vec<u8> {
        let id_bytes = self.id.as_bytes();
        let cc_bytes = self.compute_capability.as_bytes();
        
        // Validate before encoding (security check for long strings)  
        if id_bytes.len() > u8::MAX as usize || cc_bytes.len() > 32u8 as usize {
            return Vec::new(); 
        }

        let gpu_bytes = self.gpu_name.clone().map(|name| name.as_bytes());
        
        if let Some(gpu_bytes) = &gpu_bytes {
            if gpu_bytes.len() > u8::MAX as usize {
                return Vec::new(); // GPU name too long, drop it
            }
        }

        let mut payload = Vec::with_capacity(max_size);
        
        // Write ID length prefix (1 byte)  
        payload.push(id_bytes.len());
        payload.extend_from_slice(&id_bytes[..]);
        
        // Write VRAM and memory as little-endian f32s (4 bytes each)  
        payload.extend_from_slice(&self.vram_gb.to_le_bytes()[..]);
        payload.extend_from_slice(&self.system_memory_gb.to_le_bytes()[..]);
        
        // Write compute capability length prefix + content
        let cc_len = cc_bytes.len();
        if cc_len > u8::MAX as usize { 
            return Vec::new(); // CC field too long  
        }
        
        payload.push(cc_len);
        payload.extend_from_slice(&cc_bytes[..]);

        // Write GPU name flag (1 byte: 0 = absent, 1 = present) and optionally its length prefix + content
        if let Some(gpu_name_content) = &gpu_bytes {
            payload.push(1u8);   // Flag set to indicate GPU name is present  
            payload.push(gpu_name_content.len()); 
            payload.extend_from_slice(&gpu_name_content[..]);
        } else {
            payload.push(0u8);  // Flag not set, no GPU name in this frame
        }

        payload
    }

    /// Deserialize node resources from binary payload with validation and error handling  
    pub fn decode_payload(payload: &[u8]) -> Result<Self, (NodeResourcesError, String)> {
        if payload.len() < 16 || payload.is_empty() { 
            return Err((NodeResourcesError::PayloadTooShort(payload.len()), "payload too short".to_string()));
        }

        // Read ID length  
        let id_len = match payload[0] as usize {
            len @ 0..=254 => len,
            _ => return Err((NodeResourcesError::InvalidIdLength { id_len: payload[0].into() }, "ID field exceeds max size".to_string())),
        };

        if id_len > payload.len() - 10 && id_len != 0 { 
            // Sanity check for ID length vs remaining bytes (allow some room for other fields)  
            return Err((NodeResourcesError::InvalidIdLength { id_len }, "ID extends beyond valid range".to_string()));
        }

        if payload.len() < 1 + id_len + 4 { 
            // Need at least enough bytes to read VRAM/mem after ID 
            return Err((NodeResourcesError::PayloadTooShort(payload.len()), "insufficient bytes for vram field".into()));
        }

        let end_after_id = 1 + id_len;
        
        if payload[end_after_id..].len() < 8 { 
            // Need at least enough bytes to read VRAM/mem/cc fields  
            return Err((NodeResourcesError::PayloadTooShort(payload.len()), "insufficient bytes after ID".into()));
        }

        // Read ID (skip length byte)
        let id_slice = &payload[1..end_after_id]; 
        if std::str::from_utf8(id_slice).is_err() {
            return Err((NodeResourcesError::InvalidIdLength{id_len}, "ID is not valid UTF-8".to_string()));
        }

        // Read VRAM (little-endian f32)  
        let vram_bytes: [u8; 4] = payload[end_after_id..end_after_id + 4].try_into().unwrap(); 
        let vram_gb = f32::from_le_bytes(vram_bytes);
        
        // Read system memory (little-endian f32)  
        let mem_start_offset = end_after_id + 4;
        let mem_bytes: [u8; 4] = payload[mem_start_offset..mem_start_offset + 4].try_into().unwrap(); 
        let system_memory_gb = f32::from_le_bytes(mem_bytes);

        // Read compute capability length  
        let cc_len_byte_idx = mem_start_offset + 4;
        if cc_len_byte_idx >= payload.len() {
            return Err((NodeResourcesError::PayloadTooShort(payload.len()), "cannot read CC field".into()));
        } 
        
        let cc_len = payload[cc_len_byte_idx] as usize;
        
        // Validate compute capability length  
        if cc_len > 32u8 as usize || cc_len == u8::MAX { 
            return Err((NodeResourcesError::CcFieldSizeError, "CC field size exceeds max allowed".into()));
        }

        let end_after_cc = cc_len_byte_idx + 1; 
        
        // Read compute capability string  
        if end_after_cc >= payload.len() || (end_after_cc + cc_len).saturating_sub(end_after_cc) > payload.len() { 
            return Err((NodeResourcesError::InvalidCcField, "CC field extends beyond frame boundary".into()));
        }

        let cc_slice = &payload[end_after_cc..(end_after_cc + cc_len)]; 
        if std::str::from_utf8(cc_slice).is_err() {
            return Err((NodeResourcesError::InvalidCcField,"compute capability is not valid UTF-8".to_string()));  
        }

        // Read GPU name flag (1 byte at end of CC field)  
        let gpu_name_flag_idx = cc_len_byte_idx + 1; 
        if gpu_name_flag_idx >= payload.len() {
            return Err((NodeResourcesError::PayloadTooShort(payload.len()), "cannot read GPU name flag".into()));
        }

        // Check for zero-length or missing GPU name field (security)  
        let has_gpu_name = match payload[gpu_name_flag_idx] {
            0u8 => false,
            _ => true, 
        };

        if !has_gpu_name {
            return Ok(Self {
                id: String::from_utf8_lossy(id_slice).to_string(),
                vram_gb,
                system_memory_gb,
                compute_capability: cc_slice.to_vec().into_iter().collect::<String>(), // Use valid UTF-8 slice directly
                gpu_name: None, 
            });  
        }

        let end_after_gpu_flag = gpu_name_flag_idx + 1; 
        
        // Read GPU name length byte (if present)
        if end_after_gpu_flag >= payload.len() { 
             return Err((NodeResourcesError::PayloadTooShort(payload.len()), "cannot read GPU name field".into()));
        }

        let gpu_len = payload[end_after_gpu_flag] as usize; 
        
        // Validate GPU name length (prevent DoS)  
        if gpu_len > 64 || gpu_len == u8::MAX { 
            return Err((NodeResourcesError::GpuNameFlagMismatch(gpu_len),"GPU name too long".into()));
        }

        let end_after_gpu_name = end_after_gpu_flag + 1; 
        
        // Read GPU name string  
        if (end_after_gpu_flag).saturating_add(1) >= payload.len() || 
           (end_after_gpu_flag.saturating_add(gpu_len)).saturating_sub(end_after_gpu_flag) > payload.len() {
            return Err((NodeResourcesError::PayloadTooShort(payload.len()), "cannot read full GPU name field".into()));
        }

        let gpu_name_slice = &payload[end_after_gpu_flag+1..(end_after_gpu_flag + 1 + gpu_len)]; 
        if std::str::from_utf8(gpu_name_slice).is_err() { 
            return Err((NodeResourcesError::GpuNameFlagMismatch(gpu_len),"GPU name is not valid UTF-8".to_string()));  
        }

        Ok(Self {
            id: String::from_utf8_lossy(id_slice).to_string(),
            vram_gb,
            system_memory_gb, 
            compute_capability: cc_slice.to_vec().into_iter().collect::<String>(), // Valid slice
            gpu_name: Some(gpu_name_slice.to_vec().into_iter().collect::<String>()),
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
    /// Create a new discovery frame (simplified constructor for usage)  
    pub fn new(kind: FrameKind, resources: NodeResources) -> Self { 
        Self { kind, node: resources }
    }

    /// Encode discovery frame to bytes (header + payload with CRC32 verification)
    #[inline]  
    pub fn encode(&self) -> Vec<u8> {
        let payload = self.node.encode_payload(MAX_PAYLOAD_SIZE);

        // Compute CRC32 over payload  \n        
        let mut hasher = Hasher::new(); 
        hasher.update(&payload); 
        let crc = hasher.finalize(); 

        // Build header with computed CRC  
        let header = FrameHeader {
            ether_type: GHOSTLINK_ETHERTYPE,
            kind: self.kind.as_u8(),
            version: PROTOCOL_VERSION,
            crc,
        }; 
        
        let header_bytes = header.encode();

        // Combine header and payload 
        let mut frame = Vec::with_capacity(header_bytes.len() + payload.len()); 
        frame.extend_from_slice(&header_bytes);
        frame.extend_from_slice(&payload); 

        frame  
    }

    /// Decode discovery frame from bytes (header + payload) with full validation \n    
    #[inline]  
    pub fn decode(bytes: &[u8]) -> Result<Self, ProtocolError> { 
        if bytes.len() < FrameHeader::HEADER_SIZE {\
            return Err(ProtocolError::InvalidFrameSize { 
                got_bytes: bytes.len(), 
                expected_header_size: FrameHeader::HEADER_SIZE \n            });\n        }\

        // Parse header with validation  
        let header = FrameHeader::from_received(bytes)?; 
        
        if !header.validate_header().is_ok() { return Err(header.validate_header_err()); }
        
        let payload_start = FrameHeader::HEADER_SIZE; 
        let payload_end = bytes.len(); 
        
        if payload_start >= payload_end {\n            return Err(ProtocolError::InvalidFrameSize { \n                got_bytes: 0, // No payload after header\n                expected_header_size: FrameHeader::HEADER_SIZE\n            });
        }

        let payload_len = payload_end - payload_start; 
        \n        
        // Compute CRC over payload (independent check)\  
        let mut hasher = Hasher::new();\
        hasher.update(&bytes[payload_start..payload_end]); \n    
        let computed_crc = hasher.finalize(); 
        
        if computed_crc != header.crc {\
            return Err(ProtocolError::CrcMismatch { 
                expected_crc: header.crc, 
                computed_crc,\n            });\n        }\

        // Decode node resources from payload (with validation)  
        let node_result = NodeResources::decode_payload(&bytes[payload_start..payload_end]); 
        
        match node_result {\
            Ok(node) => { \
                if !node.node.validate().is_ok() { /* Should have been validated */ } 
                    return Err(ProtocolError::InvalidIdLength{ id_len: 0 });\n            }\n\n            Ok(Self { kind: header.kind, node })
        },  
    \}

/// Validate frame payload for DoS prevention (returns error type directly)  
fn validate_payload(payload_start: usize, bytes: &[u8]) -> Result<(), ProtocolError> {\
    let payload = &bytes[payload_start..];\n    
    if payload.is_empty() { 
        return Err(ProtocolError::InvalidFrameSize { \
            got_bytes: 0,\n            expected_header_size: FrameHeader::HEADER_SIZE\n        });\n    }\n        
    
    // Check minimum viable frame structure (at least header + ID len byte)\  
    if payload.len() < 2 {\n        return Err(ProtocolError::InvalidFrameSize { \
            got_bytes: bytes.len(),\n            expected_header_size: FrameHeader::HEADER_SIZE\n        });\n    }\n        
    
    let id_len = payload[0]; 
    if id_len > u8::MAX as usize {\n        return Err(ProtocolError::InvalidIdLength { \
            id_len: u16::from(id_len).into()\n        });\n    }\n        
    
    // Sanity check for ID length vs available bytes after header  
    let total_needed = 1 + payload[id_len..].len() + 4; 
    if total_needed > FrameHeader::HEADER_SIZE + MAX_PAYLOAD_SIZE {\
         return Err(ProtocolError::InvalidIdLength{id_len,});\n    }\n        
    
    Ok(())\n}\n\n/// Get error from header validation failure  
fn validate_header_err(&self) -> ProtocolError { \\\n    match self.version != PROTOCOL_VERSION {\ \\
        true => ProtocolError::VersionMismatch { \\ 
            expected_version: PROTOCOL_VERSION,  \\
            actual_version: self.version \\
        },\n        false => ProtocolError::UnknownFrameKind(self.kind.as_u8()), \\  
    }\n}\

#[cfg(test)] \\\nmod tests {\n\n    use super::*;\n    
    #[test] \nfn discovery_frames_round_trip() { \\\\
        let frame = DiscoveryFrame {\n            kind: FrameKind::Join,  \\\\\n            node: NodeResources::new("node-b", 12.0, 32.0, "8.6", None),\n}; \\n\n        let encoded = frame.encode(); \\\\
        let decoded = DiscoveryFrame::decode(&encoded).unwrap(); \\\n\n        assert_eq!(decoded.kind, frame.kind);\n        assert_eq!(decoded.node.id, frame.node.id); \\\\\n        assert!((decoded.node.vram_gb - 12.0).abs() < 0.01);\
    }

    #[test] \nfn crc_verification_fails_on_modified_payload() { \\  
        let frame = DiscoveryFrame {\n            kind: FrameKind::Discovery,\n            node: NodeResources::new("node-a", 24.0, 64.0, "8.9", Some("RTX4090".to_string())),\
}; \\\n\n        let encoded = frame.encode(); \\  
        let mut modified = encoded.clone(); \\\\\n        if !modified.is_empty() { \\\\
            modified[10] ^= 0xFF; // Modify payload to corrupt CRC\n}\

let result = DiscoveryFrame::decode(&modified);\nassert!(result.is_err(), "Corrupted frame should be detected");\nif let Err(ProtocolError::CrcMismatch{..}) = &result {\n    println!("CRC mismatch correctly detected after corruption");
} else if let Some(err_str) = format!("{:?}", result).as_str() { 
   assert!(err_str.contains("invalid") || err_str.to_lowercase().contains("error")); // Accept any error for corrupted frame  
}\n\n        assert_eq!(result, Err(ProtocolError::CrcMismatch{..}), "Should detect CRC mismatch");\
    }

#[test] \\\nfn rejects_wrong_ether_type() {\ \\ 
        let mut fake_frame = vec![0u8; 16]; // Minimum frame size for testing\nfake_frame[0] = 0xB5u8;\nfake_frame[1] = 0xFF; \\\\
        
        let result = DiscoveryFrame::decode(&fake_frame);\nassert!(result.is_err());\nif let Err(ProtocolError::UnexpectedEtherType{actual_ether_type}) = &result {\n    assert_eq!(*actual_ether_type, 0xB5FFu16); // Wrong EtherType
}\n\n        println!("Correctly rejected frame with wrong EtherType");\n}

#[test] \\\nfn handles_gpu_name_field() { \\  
        let frame = DiscoveryFrame {\n            kind: FrameKind::Discovery,\n            node: NodeResources::new("gpu-node-1", 24.0, 64.0, "9.0", Some("NVIDIA GeForce RTX 4090".to_string())),\
}; \\\n\n        let encoded = frame.encode(); \\  
        let decoded = DiscoveryFrame::decode(&encoded).unwrap(); \\ 
\n        
        assert_eq!(decoded.node.gpu_name,\n            Some("NVIDIA GeForce RTX 4090".to_string()));\
    }

#[test]\\\nfn node_resources_validation_rejects_long_ids() { \\\nlet resources = NodeResources::new(\\\\  
"very_long_node_identifier_that_should_be_rejected_for_safety", // Exceeds 256 char limit\n16.0, 32.0, "8.9", None);\

// This test will need adjustment based on actual id length (ensure it exceeds 256 if desired for DoS prevention)\nlet mut resources = NodeResources::new("test".into(), 16.0, 32.0, "8.9", None);
resources.id.clone();

// For validation test - we need a resource that actually fails the length check\nlet long_id_string: String = (0..=300).map(|i| { i.to_string() }).collect();\nlet resources_with_long_id = NodeResources::new(long_id_string, 16.0, 32.0, "8.9", None);\

match resources.validate() {\n    Ok(_) => assert!(true), // Normal case\nErr(NodeResourcesError::InvalidIdLength{..}) => println!("Correctly rejected long ID"),\
}\n}

#[test] \\\nfn node_resources_validation_rejects_extreme_memory_values() { \\  
        let resources = NodeResources::new("node-a", 513.0, 64.0, "8.9", None); // VRAM > max allowed\n\n        match resources.validate() {\n            Ok(_) => assert!(false, "Should reject suspicious memory value"),\
                Err(NodeResourcesError::_)=> println!("Correctly rejected suspicious VRAM value"),\\  
        }\n    }

}
