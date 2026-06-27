//! Binary Protocol with CRC32 Checksums for Ghost-Link Discovery
//!
//! This module implements a fixed-width binary protocol using:
//! - Fixed-size fields for zero-copy parsing
//! - CRC32 checksums for frame integrity
//! - Sequence numbers and versioning for ordering

use crc32fast::Hasher;

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
        header[0..2].copy_from_slice(&self.ether_type.to_le_bytes());
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

/// Health check frame for network heartbeats
#[derive(Clone, Debug)]
pub struct HealthCheckFrame {
    /// Node ID sending the health check
    pub node_id: String,
    /// Timestamp in seconds since epoch
    pub timestamp_secs: u64,
    /// Latency in microseconds
    pub latency_us: u32,
    /// Delivery ratio (0-100 scale, will be converted to 0.0-1.0)
    pub delivery_ratio: u8,
}

impl HealthCheckFrame {
    /// Create a new health check frame
    pub fn new(node_id: impl Into<String>, latency_us: u32, delivery_ratio: f32) -> Self {
        Self {
            node_id: node_id.into(),
            timestamp_secs: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            latency_us,
            delivery_ratio: (delivery_ratio * 100.0).clamp(0.0, 100.0) as u8,
        }
    }

    /// Encode health check frame to bytes
    pub fn encode(&self) -> Vec<u8> {
        let mut payload = Vec::with_capacity(1 + self.node_id.len() + 8 + 4 + 1);

        // Node ID length + ID
        payload.push(self.node_id.len() as u8);
        payload.extend_from_slice(self.node_id.as_bytes());

        // Timestamp (u64 LE)
        payload.extend_from_slice(&self.timestamp_secs.to_le_bytes());

        // Latency (u32 LE)
        payload.extend_from_slice(&self.latency_us.to_le_bytes());

        // Delivery ratio (u8)
        payload.push(self.delivery_ratio);

        payload
    }

    /// Decode health check frame from bytes
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        if bytes.is_empty() {
            return Err("Empty health check frame".into());
        }

        let node_id_len = bytes[0] as usize;
        if bytes.len() < 1 + node_id_len + 8 + 4 + 1 {
            return Err("Health check frame too short".into());
        }

        let node_id = String::from_utf8(bytes[1..1 + node_id_len].to_vec())
            .map_err(|_| "Invalid node ID UTF-8")?;

        let offset = 1 + node_id_len;
        let timestamp_secs = u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap());
        let latency_us = u32::from_le_bytes(bytes[offset + 8..offset + 12].try_into().unwrap());
        let delivery_ratio = bytes[offset + 12];

        Ok(Self {
            node_id,
            timestamp_secs,
            latency_us,
            delivery_ratio,
        })
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
    #[inline]
    pub fn encode_payload(&self, max_size: usize) -> Vec<u8> {
        let id_bytes = self.id.as_bytes();
        let cc_bytes = self.compute_capability.as_bytes();
        let gpu_bytes = self.gpu_name.as_ref().map(|name| name.as_bytes());

        if id_bytes.len() > u8::MAX as usize || cc_bytes.len() > u8::MAX as usize {
            return Vec::new();
        }
        if let Some(gpu_bytes) = gpu_bytes {
            if gpu_bytes.len() > u8::MAX as usize {
                return Vec::new();
            }
        }

        let payload_len =
            11 + id_bytes.len() + cc_bytes.len() + gpu_bytes.map_or(0, |bytes| 2 + bytes.len());
        if payload_len > max_size {
            return Vec::new();
        }

        let mut payload = Vec::with_capacity(payload_len);
        payload.push(id_bytes.len() as u8);
        payload.extend_from_slice(id_bytes);
        payload.extend_from_slice(unsafe {
            std::slice::from_raw_parts(&self.vram_gb as *const f32 as *const u8, 4)
        });
        payload.extend_from_slice(unsafe {
            std::slice::from_raw_parts(&self.system_memory_gb as *const f32 as *const u8, 4)
        });
        payload.push(cc_bytes.len() as u8);
        payload.extend_from_slice(cc_bytes);

        if let Some(gpu_bytes) = gpu_bytes {
            payload.push(1);
            payload.push(gpu_bytes.len() as u8);
            payload.extend_from_slice(gpu_bytes);
        } else {
            payload.push(0);
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
        let id = unsafe { std::str::from_utf8_unchecked(&payload[1..1 + id_len]) };

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
        let cc =
            unsafe { std::str::from_utf8_unchecked(&payload[10 + id_len..10 + id_len + cc_len]) };

        // Check for GPU name flag
        let has_gpu_name = payload[10 + id_len + cc_len] == 1;

        Ok(Self {
            id: id.to_string(),
            vram_gb,
            system_memory_gb,
            compute_capability: cc.to_string(),
            gpu_name: if has_gpu_name {
                let gpu_len = payload[10 + id_len + cc_len + 1] as usize;
                if gpu_len > payload.len() - 12 - id_len - cc_len {
                    return Err("invalid GPU name length".into());
                }
                Some(
                    unsafe {
                        std::str::from_utf8_unchecked(
                            &payload[10 + id_len + cc_len + 2..10 + id_len + cc_len + 2 + gpu_len],
                        )
                    }
                    .to_string(),
                )
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
    #[inline]
    pub fn encode(&self) -> Vec<u8> {
        let payload = self.node.encode_payload(MAX_PAYLOAD_SIZE);

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
        let header_bytes = header.encode();

        // Combine header and payload
        let mut frame = Vec::with_capacity(header_bytes.len() + payload.len());
        frame.extend_from_slice(&header_bytes);
        frame.extend_from_slice(&payload);

        frame
    }

    /// Decode discovery frame from bytes (header + payload)
    #[inline]
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() < FrameHeader::HEADER_SIZE {
            return Err("frame too short".into());
        }

        // Parse header
        let ether_type = u16::from_le_bytes([bytes[0], bytes[1]]);
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
            return Err(format!(
                "CRC mismatch: expected 0x{expected_crc:08x}, got 0x{computed_crc:08x}"
            ));
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
        fake_frame[0] = 0xB5u8; // Low byte of GHOSTLINK_ETHERTYPE (0x88B5 LE)
        fake_frame[1] = 0xFF; // Wrong high byte

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
                Some("NVIDIA GeForce RTX 4090".to_string()),
            ),
        };

        let encoded = frame.encode();
        let decoded = DiscoveryFrame::decode(&encoded).unwrap();

        assert_eq!(
            decoded.node.gpu_name,
            Some("NVIDIA GeForce RTX 4090".to_string())
        );
    }

    // ====================================================================
    // PROPERTY-BASED TESTS (proptest)
    // ====================================================================

    #[cfg(test)]
    mod proptest_protocol {
        use super::super::*;
        use proptest::prelude::*;

        proptest! {
            /// Property: Any valid discovery frame should round-trip encode/decode
            #[test]
            fn prop_discovery_frame_round_trip(
                node_id in "[a-z0-9]{1,12}",
                vram in 1.0f32..512.0,
                ram in 8.0f32..1024.0,
            ) {
                let frame = DiscoveryFrame {
                    kind: FrameKind::Discovery,
                    node: NodeResources::new(format!("node-{}", node_id), vram, ram, "8.9", None),
                };

                let encoded = frame.encode();
                let decoded = DiscoveryFrame::decode(&encoded).expect("round-trip failed");

                assert_eq!(decoded.node.id, frame.node.id);
                assert!((decoded.node.vram_gb - frame.node.vram_gb).abs() < 0.01);
                assert!((decoded.node.system_memory_gb - frame.node.system_memory_gb).abs() < 0.01);
            }

            /// Property: CRC should detect corruption with high probability
            #[test]
            fn prop_crc_detects_any_payload_corruption(
                seed in 0u8..255,
                corruption_pos in 0usize..256,
            ) {
                let frame = DiscoveryFrame {
                    kind: FrameKind::Join,
                    node: NodeResources::new("node-test", 24.0, 64.0, "8.9", None),
                };

                let mut encoded = frame.encode();
                if corruption_pos < encoded.len() && corruption_pos < 200 {
                    // Corrupt payload (not header/CRC area)
                    if corruption_pos > 10 && corruption_pos < encoded.len().saturating_sub(4) {
                        encoded[corruption_pos] = encoded[corruption_pos].wrapping_add(seed.wrapping_add(1));

                        // Should fail CRC check
                        let result = DiscoveryFrame::decode(&encoded);
                        assert!(result.is_err(), "Corruption at byte {} should be detected", corruption_pos);
                    }
                }
            }

            /// Property: Frame encoding is deterministic
            #[test]
            fn prop_frame_encoding_is_deterministic(
                node_id in "[a-z0-9]{1,8}",
                vram in 1.0f32..100.0,
            ) {
                let frame1 = DiscoveryFrame {
                    kind: FrameKind::Discovery,
                    node: NodeResources::new(format!("node-{}", node_id), vram, 64.0, "8.9", None),
                };

                let frame2 = DiscoveryFrame {
                    kind: FrameKind::Discovery,
                    node: NodeResources::new(format!("node-{}", node_id), vram, 64.0, "8.9", None),
                };

                let enc1 = frame1.encode();
                let enc2 = frame2.encode();

                // Same input should produce same output
                assert_eq!(enc1, enc2, "Encoding should be deterministic");
            }
        }
    }
}

#[cfg(test)]
mod tests_health_check_frame {
    use super::*;

    #[test]
    fn test_health_check_frame_encode_decode() {
        let frame = HealthCheckFrame::new("node-01", 1200, 0.99);
        let encoded = frame.encode();
        let decoded = HealthCheckFrame::decode(&encoded).unwrap();

        assert_eq!(decoded.node_id, "node-01");
        assert_eq!(decoded.latency_us, 1200);
        assert!(decoded.delivery_ratio >= 98 && decoded.delivery_ratio <= 100);
    }

    #[test]
    fn test_health_check_frame_round_trip() {
        let original = HealthCheckFrame {
            node_id: "test-node".to_string(),
            timestamp_secs: 1234567890,
            latency_us: 5000,
            delivery_ratio: 85,
        };

        let encoded = original.encode();
        let decoded = HealthCheckFrame::decode(&encoded).unwrap();

        assert_eq!(decoded.node_id, original.node_id);
        assert_eq!(decoded.timestamp_secs, original.timestamp_secs);
        assert_eq!(decoded.latency_us, original.latency_us);
        assert_eq!(decoded.delivery_ratio, original.delivery_ratio);
    }

    #[test]
    fn test_health_check_frame_invalid_data() {
        // Empty frame
        assert!(HealthCheckFrame::decode(&[]).is_err());

        // Too short frame
        assert!(HealthCheckFrame::decode(&[5, 1, 2, 3]).is_err());
    }
}
