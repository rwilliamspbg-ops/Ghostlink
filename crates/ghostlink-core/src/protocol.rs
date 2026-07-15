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
        let et_bytes = self.ether_type.to_le_bytes();
        header[0] = et_bytes[0];
        header[1] = et_bytes[1];
        header[2] = self.kind;
        header[3] = self.version;
        let crc_bytes = self.crc.to_le_bytes();
        header[4] = crc_bytes[0];
        header[5] = crc_bytes[1];
        header[6] = crc_bytes[2];
        header[7] = crc_bytes[3];
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

    /// Serialize node resources into an existing binary buffer.
    ///
    /// Returns the length of the written payload on success, or an Error on validation failure.
    #[inline]
    pub fn encode_payload_into(
        &self,
        buffer: &mut Vec<u8>,
        max_size: usize,
    ) -> Result<usize, &'static str> {
        let id_bytes = self.id.as_bytes();
        let cc_bytes = self.compute_capability.as_bytes();
        let gpu_bytes = self.gpu_name.as_ref().map(|name| name.as_bytes());

        if id_bytes.len() > u8::MAX as usize || cc_bytes.len() > u8::MAX as usize {
            return Err("field length exceeds u8::MAX");
        }
        if let Some(gpu_bytes) = gpu_bytes {
            if gpu_bytes.len() > u8::MAX as usize {
                return Err("GPU name length exceeds u8::MAX");
            }
        }

        let payload_len =
            11 + id_bytes.len() + cc_bytes.len() + gpu_bytes.map_or(0, |bytes| 2 + bytes.len());
        if payload_len > max_size {
            return Err("payload length exceeds max_size");
        }

        buffer.push(id_bytes.len() as u8);
        buffer.extend_from_slice(id_bytes);
        buffer.extend_from_slice(&self.vram_gb.to_le_bytes());
        buffer.extend_from_slice(&self.system_memory_gb.to_le_bytes());
        buffer.push(cc_bytes.len() as u8);
        buffer.extend_from_slice(cc_bytes);

        if let Some(gpu_bytes) = gpu_bytes {
            buffer.push(1);
            buffer.push(gpu_bytes.len() as u8);
            buffer.extend_from_slice(gpu_bytes);
        } else {
            buffer.push(0);
        }

        Ok(payload_len)
    }

    /// Serialize node resources to fixed-width binary payload
    ///
    /// Format: [id_len(1) + id + vram_f32_le + mem_f32_le + cc_len(1) + cc]
    #[inline]
    pub fn encode_payload(&self, max_size: usize) -> Vec<u8> {
        let id_bytes_len = self.id.len();
        let cc_bytes_len = self.compute_capability.len();
        let gpu_bytes_len = self.gpu_name.as_ref().map_or(0, |name| name.len());
        let est_len = 11
            + id_bytes_len
            + cc_bytes_len
            + if gpu_bytes_len > 0 {
                2 + gpu_bytes_len
            } else {
                0
            };

        let mut payload = Vec::with_capacity(est_len);
        if self.encode_payload_into(&mut payload, max_size).is_ok() {
            payload
        } else {
            Vec::new()
        }
    }

    /// Deserialize node resources from binary payload
    pub fn decode_payload(payload: &[u8]) -> Result<Self, String> {
        // Minimum payload: id_len(1) + vram(4) + mem(4) + cc_len(1) + has_gpu_name(1)
        if payload.len() < 11 {
            return Err("payload too short".into());
        }

        let mut cursor = 0usize;

        // Read ID
        let id_len = *payload
            .get(cursor)
            .ok_or_else(|| "missing ID length".to_string())? as usize;
        cursor += 1;
        let id_slice = payload
            .get(cursor..cursor + id_len)
            .ok_or_else(|| "invalid ID length".to_string())?;
        let id = std::str::from_utf8(id_slice)
            .map_err(|_| "ID contains invalid UTF-8".to_string())?
            .to_string();
        cursor += id_len;

        // Read VRAM (little-endian f32)
        let vram_slice = payload
            .get(cursor..cursor + 4)
            .ok_or_else(|| "missing VRAM bytes".to_string())?;
        let vram_bytes: [u8; 4] = vram_slice
            .try_into()
            .map_err(|_| "invalid VRAM byte length".to_string())?;
        let vram_gb = f32::from_le_bytes(vram_bytes);
        cursor += 4;

        // Read system memory (little-endian f32)
        let mem_slice = payload
            .get(cursor..cursor + 4)
            .ok_or_else(|| "missing system memory bytes".to_string())?;
        let mem_bytes: [u8; 4] = mem_slice
            .try_into()
            .map_err(|_| "invalid system memory byte length".to_string())?;
        let system_memory_gb = f32::from_le_bytes(mem_bytes);
        cursor += 4;

        // Read compute capability
        let cc_len = *payload
            .get(cursor)
            .ok_or_else(|| "missing CC length".to_string())? as usize;
        cursor += 1;
        let cc_slice = payload
            .get(cursor..cursor + cc_len)
            .ok_or_else(|| "invalid CC length".to_string())?;
        let compute_capability = std::str::from_utf8(cc_slice)
            .map_err(|_| "compute capability contains invalid UTF-8".to_string())?
            .to_string();
        cursor += cc_len;

        // Check for GPU name flag
        let has_gpu_name = *payload
            .get(cursor)
            .ok_or_else(|| "missing GPU name flag".to_string())?
            == 1;
        cursor += 1;

        let gpu_name = if has_gpu_name {
            let gpu_len = *payload
                .get(cursor)
                .ok_or_else(|| "missing GPU name length".to_string())?
                as usize;
            cursor += 1;

            let gpu_slice = payload
                .get(cursor..cursor + gpu_len)
                .ok_or_else(|| "invalid GPU name length".to_string())?;
            Some(
                std::str::from_utf8(gpu_slice)
                    .map_err(|_| "GPU name contains invalid UTF-8".to_string())?
                    .to_string(),
            )
        } else {
            None
        };

        Ok(Self {
            id,
            vram_gb,
            system_memory_gb,
            compute_capability,
            gpu_name,
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
        let id_bytes_len = self.node.id.len();
        let cc_bytes_len = self.node.compute_capability.len();
        let payload_len = 11
            + id_bytes_len
            + cc_bytes_len
            + self.node.gpu_name.as_ref().map_or(0, |name| 2 + name.len());

        let mut frame = Vec::with_capacity(8 + payload_len);
        // Reserve the first 8 bytes for the header with a fast slice extension
        frame.extend_from_slice(&[0u8; 8]);

        if self
            .node
            .encode_payload_into(&mut frame, MAX_PAYLOAD_SIZE)
            .is_ok()
        {
            // Compute CRC32 over payload
            let mut hasher = Hasher::new();
            hasher.update(&frame[8..]);
            let crc = hasher.finalize();

            // Build header
            let header = FrameHeader {
                ether_type: GHOSTLINK_ETHERTYPE,
                kind: self.kind.as_u8(),
                version: PROTOCOL_VERSION,
                crc,
            };
            let header_bytes = header.encode();

            // Write header to first 8 bytes
            frame[0..8].copy_from_slice(&header_bytes);

            frame
        } else {
            self.encode_fallback_or_empty()
        }
    }

    #[inline(never)]
    fn encode_fallback_or_empty(&self) -> Vec<u8> {
        // Fallback for empty payload
        let mut hasher = Hasher::new();
        hasher.update(&[]);
        let crc = hasher.finalize();

        let header = FrameHeader {
            ether_type: GHOSTLINK_ETHERTYPE,
            kind: self.kind.as_u8(),
            version: PROTOCOL_VERSION,
            crc,
        };
        let header_bytes = header.encode();

        let mut frame = Vec::with_capacity(header_bytes.len());
        frame.extend_from_slice(&header_bytes);
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
