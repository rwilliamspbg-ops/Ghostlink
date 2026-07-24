//! Binary Protocol with CRC32 Checksums for Ghost-Link Discovery
//!
//! This module implements a fixed-width binary protocol using:
//! - Fixed-size fields for zero-copy parsing
//! - CRC32 checksums for frame integrity (hardware-accelerated via crc32fast)
//! - Sequence numbers and versioning for ordering
//! - Zero-copy encoding with thread-local buffers

use crc32fast::Hasher;

/// Ghost-Link EtherType (0x88B5)
pub const GHOSTLINK_ETHERTYPE: u16 = 0x88B5;

/// Protocol version
pub const PROTOCOL_VERSION: u8 = 1;

/// Maximum payload size for discovery frames
pub const MAX_PAYLOAD_SIZE: usize = 256;

/// Maximum frame size (header + max payload)
pub const MAX_FRAME_SIZE: usize = 8 + MAX_PAYLOAD_SIZE;

/// Frame header structure (fixed-width, 8 bytes)
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
    const HEADER_SIZE: usize = 8;

    /// Create a new frame header with computed CRC from pre-encoded payload
    #[inline]
    pub fn new(ether_type: u16, kind: u8, payload: &[u8]) -> Self {
        let crc = crc32(payload);
        Self {
            ether_type,
            kind,
            version: PROTOCOL_VERSION,
            crc,
        }
    }

    /// Encode header directly into a pre-allocated buffer (zero-copy)
    #[inline]
    pub fn encode_into(&self, buf: &mut [u8; Self::HEADER_SIZE]) {
        let et_bytes = self.ether_type.to_le_bytes();
        buf[0] = et_bytes[0];
        buf[1] = et_bytes[1];
        buf[2] = self.kind;
        buf[3] = self.version;
        let crc_bytes = self.crc.to_le_bytes();
        buf[4] = crc_bytes[0];
        buf[5] = crc_bytes[1];
        buf[6] = crc_bytes[2];
        buf[7] = crc_bytes[3];
    }

    /// Decode header from bytes (zero-copy, no allocation)
    #[inline]
    pub fn decode(buf: &[u8]) -> Option<Self> {
        if buf.len() < Self::HEADER_SIZE {
            return None;
        }
        let ether_type = u16::from_le_bytes([buf[0], buf[1]]);
        let kind = buf[2];
        let version = buf[3];
        let crc = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]);
        Some(Self {
            ether_type,
            kind,
            version,
            crc,
        })
    }
}

/// Compute CRC32 checksum using hardware-accelerated crc32fast
#[inline]
pub fn crc32(data: &[u8]) -> u32 {
    let mut hasher = Hasher::new();
    hasher.update(data);
    hasher.finalize()
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
            11 + id_bytes.len() + cc_bytes.len() + gpu_bytes.map_or(0, |bytes| 1 + bytes.len());
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
        let len = payload.len();
        if len < 11 {
            return Err("payload too short".into());
        }

        let mut cursor = 0usize;

        let id_len = payload[cursor] as usize;
        cursor += 1;
        let id_end = cursor + id_len;
        if id_end > len {
            return Err("payload truncated".into());
        }
        let id = String::from_utf8(payload[cursor..id_end].to_vec())
            .map_err(|_| "invalid UTF-8 in ID".to_string())?;
        cursor = id_end;

        let vram_end = cursor + 8;
        if vram_end > len {
            return Err("payload truncated".into());
        }
        let vram_gb = f32::from_le_bytes([
            payload[cursor],
            payload[cursor + 1],
            payload[cursor + 2],
            payload[cursor + 3],
        ]);
        let system_memory_gb = f32::from_le_bytes([
            payload[cursor + 4],
            payload[cursor + 5],
            payload[cursor + 6],
            payload[cursor + 7],
        ]);
        cursor = vram_end;

        if cursor >= len {
            return Err("payload truncated".into());
        }
        let cc_len = payload[cursor] as usize;
        cursor += 1;
        let cc_end = cursor + cc_len;
        if cc_end > len {
            return Err("payload truncated".into());
        }
        let compute_capability = String::from_utf8(payload[cursor..cc_end].to_vec())
            .map_err(|_| "invalid UTF-8 in CC".to_string())?;
        cursor = cc_end;

        if cursor >= len {
            return Err("payload truncated".into());
        }
        let has_gpu = payload[cursor] == 1;
        cursor += 1;

        let gpu_name = if has_gpu {
            if cursor >= len {
                return Err("payload truncated".into());
            }
            let gpu_len = payload[cursor] as usize;
            cursor += 1;
            let gpu_end = cursor + gpu_len;
            if gpu_end > len {
                return Err("payload truncated".into());
            }
            Some(
                String::from_utf8(payload[cursor..gpu_end].to_vec())
                    .map_err(|_| "invalid UTF-8 in GPU name".to_string())?,
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
    /// Encode discovery frame to bytes using a stack-allocated fixed-size buffer.
    /// Avoids all Vec capacity checks and `extend_from_slice` overhead.
    #[inline]
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = [0u8; MAX_FRAME_SIZE];
        let et = GHOSTLINK_ETHERTYPE.to_le_bytes();
        buf[0] = et[0];
        buf[1] = et[1];
        buf[2] = self.kind.as_u8();
        buf[3] = PROTOCOL_VERSION;

        let id_bytes = self.node.id.as_bytes();
        let cc_bytes = self.node.compute_capability.as_bytes();

        if id_bytes.len() > u8::MAX as usize || cc_bytes.len() > u8::MAX as usize {
            buf[4..8].copy_from_slice(&crc32(&[]).to_le_bytes());
            return buf[..8].to_vec();
        }
        if let Some(ref gpu_name) = self.node.gpu_name {
            if gpu_name.len() > u8::MAX as usize {
                buf[4..8].copy_from_slice(&crc32(&[]).to_le_bytes());
                return buf[..8].to_vec();
            }
        }

        // Guard against the combined fields overflowing the fixed-size stack
        // buffer: each field is individually bounded by u8::MAX above, but
        // their sum can still exceed MAX_PAYLOAD_SIZE and panic on the
        // slice-index writes below if left unchecked.
        let gpu_len = self.node.gpu_name.as_ref().map(|name| name.len());
        let payload_len = 11 + id_bytes.len() + cc_bytes.len() + gpu_len.map_or(0, |len| 1 + len);
        if payload_len > MAX_PAYLOAD_SIZE {
            buf[4..8].copy_from_slice(&crc32(&[]).to_le_bytes());
            return buf[..8].to_vec();
        }

        let mut pos = 8usize;

        buf[pos] = id_bytes.len() as u8;
        pos += 1;
        buf[pos..pos + id_bytes.len()].copy_from_slice(id_bytes);
        pos += id_bytes.len();

        buf[pos..pos + 4].copy_from_slice(&self.node.vram_gb.to_le_bytes());
        pos += 4;

        buf[pos..pos + 4].copy_from_slice(&self.node.system_memory_gb.to_le_bytes());
        pos += 4;

        buf[pos] = cc_bytes.len() as u8;
        pos += 1;
        buf[pos..pos + cc_bytes.len()].copy_from_slice(cc_bytes);
        pos += cc_bytes.len();

        if let Some(ref gpu_name) = self.node.gpu_name {
            let gpu_bytes = gpu_name.as_bytes();
            buf[pos] = 1;
            pos += 1;
            buf[pos] = gpu_bytes.len() as u8;
            pos += 1;
            buf[pos..pos + gpu_bytes.len()].copy_from_slice(gpu_bytes);
            pos += gpu_bytes.len();
        } else {
            buf[pos] = 0;
            pos += 1;
        }

        let crc = crc32(&buf[8..pos]);
        buf[4..8].copy_from_slice(&crc.to_le_bytes());

        buf[..pos].to_vec()
    }

    /// Encode discovery frame into a pre-allocated buffer (zero-copy hot path)
    /// Returns the number of bytes written.
    #[inline]
    pub fn encode_into(&self, buf: &mut Vec<u8>) -> usize {
        buf.clear();
        buf.extend_from_slice(&[0u8; 8]);

        if self.node.encode_payload_into(buf, MAX_PAYLOAD_SIZE).is_ok() {
            let payload = &buf[8..];
            let crc = crc32(payload);

            let header = FrameHeader {
                ether_type: GHOSTLINK_ETHERTYPE,
                kind: self.kind.as_u8(),
                version: PROTOCOL_VERSION,
                crc,
            };
            header.encode_into(unsafe { &mut *(buf.as_mut_ptr() as *mut [u8; 8]) });

            buf.len()
        } else {
            buf.clear();
            buf.extend_from_slice(&[0u8; 8]);
            let header = FrameHeader {
                ether_type: GHOSTLINK_ETHERTYPE,
                kind: self.kind.as_u8(),
                version: PROTOCOL_VERSION,
                crc: crc32(&[]),
            };
            header.encode_into(unsafe { &mut *(buf.as_mut_ptr() as *mut [u8; 8]) });
            buf.len()
        }
    }

    /// Decode discovery frame from bytes (header + payload) - optimized path
    #[inline]
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() < FrameHeader::HEADER_SIZE {
            return Err("frame too short".into());
        }

        // Fast header decode using zero-copy method
        let header = FrameHeader::decode(bytes).ok_or("invalid header")?;

        if header.ether_type != GHOSTLINK_ETHERTYPE {
            return Err(format!("unexpected EtherType 0x{:04x}", header.ether_type));
        }

        if header.version != PROTOCOL_VERSION {
            return Err(format!("unsupported protocol version {}", header.version));
        }

        // Fast CRC check
        let payload = &bytes[FrameHeader::HEADER_SIZE..];
        let computed_crc = crc32(payload);

        if computed_crc != header.crc {
            return Err(format!(
                "CRC mismatch: expected 0x{:08x}, got 0x{:08x}",
                header.crc, computed_crc
            ));
        }

        // Decode node resources from payload
        let node = NodeResources::decode_payload(payload)?;

        Ok(Self {
            kind: FrameKind::try_from(header.kind)?,
            node,
        })
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
    fn encode_payload_into_accepts_exact_max_size_with_gpu_name() {
        // Regression test: encode_payload_into previously overcounted the
        // gpu-name field length by 1 byte, causing it to reject payloads
        // that actually fit exactly within max_size.
        let node = NodeResources::new("a", 1.0, 1.0, "b", Some("c".to_string()));
        let mut buffer = Vec::new();
        // Actual encoded length: id_len(1)+id(1)+vram(4)+mem(4)+cc_len(1)+cc(1)
        // +has_gpu(1)+gpu_len(1)+gpu(1) = 15 bytes.
        let written = node
            .encode_payload_into(&mut buffer, 15)
            .expect("payload of exactly 15 bytes must fit within max_size 15");
        assert_eq!(written, 15);
        assert_eq!(buffer.len(), 15);
    }

    #[test]
    fn encode_does_not_panic_when_combined_field_lengths_exceed_max_payload() {
        // Regression test: DiscoveryFrame::encode only validated each field
        // individually against u8::MAX, so combined field lengths could
        // exceed MAX_PAYLOAD_SIZE and panic on the fixed-size stack buffer
        // writes. It should now fall back to a safe header-only frame.
        let big_id = "x".repeat(200);
        let big_cc = "y".repeat(200);
        let frame = DiscoveryFrame {
            kind: FrameKind::Discovery,
            node: NodeResources::new(big_id, 1.0, 1.0, big_cc, None),
        };

        let encoded = frame.encode();
        assert_eq!(encoded.len(), 8, "should fall back to header-only frame");

        let decoded_err = DiscoveryFrame::decode(&encoded).unwrap_err();
        assert!(decoded_err.contains("payload too short") || decoded_err.contains("truncated"));
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
                    node: NodeResources::new(format!("node-{node_id}"), vram, ram, "8.9", None),
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
                        assert!(result.is_err(), "Corruption at byte {corruption_pos} should be detected");
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
                    node: NodeResources::new(format!("node-{node_id}"), vram, 64.0, "8.9", None),
                };

                let frame2 = DiscoveryFrame {
                    kind: FrameKind::Discovery,
                    node: NodeResources::new(format!("node-{node_id}"), vram, 64.0, "8.9", None),
                };

                let enc1 = frame1.encode();
                let enc2 = frame2.encode();

                // Same input should produce same output
                assert_eq!(enc1, enc2, "Encoding should be deterministic");
            }
        }
    }
}
