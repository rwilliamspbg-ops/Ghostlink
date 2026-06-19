use crate::cluster::NodeResources;

pub const GHOSTLINK_ETHERTYPE: u16 = 0x88B5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameKind {
    Discovery = 1,
    Join = 2,
    Attestation = 3,
}

impl FrameKind {
    fn from_u8(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Discovery),
            2 => Some(Self::Join),
            3 => Some(Self::Attestation),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DiscoveryFrame {
    pub kind: FrameKind,
    pub node: NodeResources,
}

impl DiscoveryFrame {
    pub fn encode(&self) -> Vec<u8> {
        let payload = format!(
            "{}|{}|{}|{}",
            self.node.id,
            self.node.vram_gb,
            self.node.system_memory_gb,
            self.node.compute_capability
        );
        let mut bytes = Vec::with_capacity(3 + payload.len());
        bytes.extend_from_slice(&GHOSTLINK_ETHERTYPE.to_be_bytes());
        bytes.push(self.kind as u8);
        bytes.extend_from_slice(payload.as_bytes());
        bytes
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() < 3 {
            return Err("frame too short".into());
        }

        let ether_type = u16::from_be_bytes([bytes[0], bytes[1]]);
        if ether_type != GHOSTLINK_ETHERTYPE {
            return Err(format!("unexpected EtherType 0x{ether_type:04x}"));
        }

        let kind = FrameKind::from_u8(bytes[2]).ok_or("unknown frame kind")?;
        let payload = std::str::from_utf8(&bytes[3..]).map_err(|error| error.to_string())?;
        let mut parts = payload.split('|');
        let id = parts.next().ok_or("missing node id")?;
        let vram_gb = parts
            .next()
            .ok_or("missing VRAM")?
            .parse::<f32>()
            .map_err(|error| error.to_string())?;
        let system_memory_gb = parts
            .next()
            .ok_or("missing system memory")?
            .parse::<f32>()
            .map_err(|error| error.to_string())?;
        let compute_capability = parts.next().ok_or("missing compute capability")?;

        Ok(Self {
            kind,
            node: NodeResources::new(id, vram_gb, system_memory_gb, compute_capability),
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
            node: NodeResources::new("node-b", 12.0, 32.0, "8.6"),
        };

        let decoded = DiscoveryFrame::decode(&frame.encode()).unwrap();

        assert_eq!(decoded, frame);
    }
}
