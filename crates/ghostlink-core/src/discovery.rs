//! UDP discovery fallback for mixed LAN environments.
//!
//! This module provides a lightweight broadcast path that can coexist with
//! raw L2 discovery. Frames retain the same binary payload format.

use std::io;
use std::net::{SocketAddr, UdpSocket};
use std::time::{SystemTime, UNIX_EPOCH};
use std::time::{Duration, Instant};

use hmac::{Hmac, Mac};
use rand::rngs::OsRng;
use rand::RngCore;
use sha2::Sha256;

use crc32fast::Hasher;

use crate::protocol::{DiscoveryFrame, FrameKind, NodeResources};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DiscoveryDropCounters {
    pub malformed: usize,
    pub auth_mismatch: usize,
    pub unsupported_kind: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DiscoveryServeStats {
    pub replies_sent: usize,
    pub drops: DiscoveryDropCounters,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum DatagramDecodeError {
    DatagramTooShortForAuth,
    InvalidAuthTrailerLength,
    UnsupportedAuthProtocolVersion(u8),
    AuthTimestampOutsideWindow,
    ReplayNonceDetected,
    AuthTagMismatch,
    FrameDecode(String),
}

impl std::fmt::Display for DatagramDecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DatagramTooShortForAuth => {
                write!(f, "discovery datagram too short for auth trailer")
            }
            Self::InvalidAuthTrailerLength => write!(f, "invalid auth trailer length"),
            Self::UnsupportedAuthProtocolVersion(version) => {
                write!(f, "unsupported discovery auth protocol version {}", version)
            }
            Self::AuthTimestampOutsideWindow => {
                write!(f, "discovery datagram auth timestamp outside allowed window")
            }
            Self::ReplayNonceDetected => {
                write!(f, "discovery datagram replay nonce detected")
            }
            Self::AuthTagMismatch => write!(f, "discovery datagram auth tag mismatch"),
            Self::FrameDecode(message) => write!(f, "{message}"),
        }
    }
}

const DISCOVERY_AUTH_PROTOCOL_VERSION: u8 = 2;
const DISCOVERY_AUTH_MARKER: [u8; 2] = *b"GL";
const DISCOVERY_AUTH_TAG_LEN: usize = 32;
const DISCOVERY_NONCE_LEN: usize = 16;
const DISCOVERY_AUTH_TRAILER_LEN: usize =
    2 + 1 + 8 + DISCOVERY_NONCE_LEN + DISCOVERY_AUTH_TAG_LEN;
const DISCOVERY_AUTH_MAX_SKEW_SECS: u64 = 30;
const DISCOVERY_NONCE_CACHE_CAPACITY: usize = 4096;

type HmacSha256 = Hmac<Sha256>;

#[derive(Default)]
struct ReplayCache {
    nonces: std::collections::HashSet<u128>,
    order: std::collections::VecDeque<u128>,
}

impl ReplayCache {
    fn check_and_insert(&mut self, nonce: u128) -> bool {
        if self.nonces.contains(&nonce) {
            return false;
        }

        self.nonces.insert(nonce);
        self.order.push_back(nonce);
        while self.order.len() > DISCOVERY_NONCE_CACHE_CAPACITY {
            if let Some(evicted) = self.order.pop_front() {
                self.nonces.remove(&evicted);
            }
        }

        true
    }
}

/// Default UDP discovery port.
pub const DEFAULT_DISCOVERY_PORT: u16 = 45885;

/// UDP fallback configuration for discovery broadcast and reply collection.
#[derive(Clone, Debug)]
pub struct UdpDiscoveryConfig {
    /// Local bind address used for sending and receiving replies.
    pub bind_addr: SocketAddr,
    /// Target broadcast address for join/discovery datagrams.
    pub broadcast_addr: SocketAddr,
    /// How long to wait for discovery responses.
    pub response_timeout: Duration,
    /// Optional shared token used to authenticate datagrams.
    pub auth_token: Option<String>,
    /// Allow decode fallback to legacy CRC32 auth tags during migration.
    pub allow_legacy_crc32: bool,
    /// Maximum datagram size expected during receive.
    pub max_datagram_size: usize,
}

impl Default for UdpDiscoveryConfig {
    fn default() -> Self {
        Self {
            bind_addr: SocketAddr::from(([0, 0, 0, 0], 0)),
            broadcast_addr: SocketAddr::from(([255, 255, 255, 255], DEFAULT_DISCOVERY_PORT)),
            response_timeout: Duration::from_millis(750),
            auth_token: None,
            allow_legacy_crc32: false,
            max_datagram_size: 2048,
        }
    }
}

/// Broadcast a discovery frame over UDP and collect any valid replies.
pub fn broadcast_and_collect(
    frame: &DiscoveryFrame,
    config: &UdpDiscoveryConfig,
) -> Result<Vec<DiscoveryFrame>, String> {
    let socket = UdpSocket::bind(config.bind_addr)
        .map_err(|e| format!("failed to bind UDP discovery socket: {e}"))?;
    socket
        .set_broadcast(true)
        .map_err(|e| format!("failed to enable UDP broadcast: {e}"))?;
    socket
        .set_read_timeout(Some(Duration::from_millis(100)))
        .map_err(|e| format!("failed to configure UDP read timeout: {e}"))?;

    let packet = encode_datagram(frame, config.auth_token.as_deref());
    socket
        .send_to(&packet, config.broadcast_addr)
        .map_err(|e| format!("failed to send UDP discovery datagram: {e}"))?;

    let mut peers = Vec::new();
    let mut recv_buf = vec![0_u8; config.max_datagram_size.max(256)];
    let deadline = Instant::now() + config.response_timeout;
    let mut replay_cache = ReplayCache::default();

    while Instant::now() < deadline {
        match socket.recv_from(&mut recv_buf) {
            Ok((read, _addr)) => {
                let datagram = &recv_buf[..read];
                if let Ok(decoded) = decode_datagram_with_options(
                    datagram,
                    config.auth_token.as_deref(),
                    config.allow_legacy_crc32,
                    Some(&mut replay_cache),
                ) {
                    peers.push(decoded);
                }
            }
            Err(err)
                if matches!(
                    err.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) => {}
            Err(err) => return Err(format!("failed while receiving UDP discovery reply: {err}")),
        }
    }

    Ok(peers)
}

/// Listen for one discovery request and reply with local node resources.
///
/// Returns the peer socket address that was replied to when a valid request is
/// received before timeout, or `None` when the wait timed out.
pub fn respond_once(
    local_node: &NodeResources,
    config: &UdpDiscoveryConfig,
) -> Result<Option<SocketAddr>, String> {
    let socket = UdpSocket::bind(config.bind_addr)
        .map_err(|e| format!("failed to bind UDP discovery listener socket: {e}"))?;
    socket
        .set_read_timeout(Some(Duration::from_millis(100)))
        .map_err(|e| format!("failed to configure UDP listener timeout: {e}"))?;

    let mut recv_buf = vec![0_u8; config.max_datagram_size.max(256)];
    let deadline = Instant::now() + config.response_timeout;
    let mut replay_cache = ReplayCache::default();

    while Instant::now() < deadline {
        let (read, peer_addr) = match socket.recv_from(&mut recv_buf) {
            Ok(parts) => parts,
            Err(err)
                if matches!(
                    err.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                continue;
            }
            Err(err) => {
                return Err(format!(
                    "failed while waiting for UDP discovery datagram: {err}"
                ));
            }
        };

        let Ok(incoming) = decode_datagram_with_options(
            &recv_buf[..read],
            config.auth_token.as_deref(),
            config.allow_legacy_crc32,
            Some(&mut replay_cache),
        ) else {
            // Ignore malformed/auth-mismatched datagrams and keep listening.
            continue;
        };

        if !matches!(incoming.kind, FrameKind::Join | FrameKind::Discovery) {
            continue;
        }

        let reply = DiscoveryFrame {
            kind: FrameKind::Discovery,
            node: local_node.clone(),
        };
        let encoded = encode_datagram(&reply, config.auth_token.as_deref());
        socket
            .send_to(&encoded, peer_addr)
            .map_err(|e| format!("failed to send UDP discovery reply: {e}"))?;

        return Ok(Some(peer_addr));
    }

    Ok(None)
}

/// Serve discovery requests and respond with local node resources.
///
/// When `max_replies` is `Some(n)`, the function returns after `n` successful
/// replies. When `None`, it runs indefinitely.
pub fn serve_discovery(
    local_node: &NodeResources,
    config: &UdpDiscoveryConfig,
    max_replies: Option<usize>,
) -> Result<usize, String> {
    serve_discovery_with_stats(local_node, config, max_replies).map(|stats| stats.replies_sent)
}

pub fn serve_discovery_with_stats(
    local_node: &NodeResources,
    config: &UdpDiscoveryConfig,
    max_replies: Option<usize>,
) -> Result<DiscoveryServeStats, String> {
    let socket = UdpSocket::bind(config.bind_addr)
        .map_err(|e| format!("failed to bind UDP discovery listener socket: {e}"))?;
    socket
        .set_read_timeout(Some(Duration::from_millis(250)))
        .map_err(|e| format!("failed to configure UDP listener timeout: {e}"))?;

    let reply = DiscoveryFrame {
        kind: FrameKind::Discovery,
        node: local_node.clone(),
    };
    let reply_bytes = encode_datagram(&reply, config.auth_token.as_deref());

    let mut stats = DiscoveryServeStats::default();
    let mut recv_buf = vec![0_u8; config.max_datagram_size.max(256)];
    let mut replay_cache = ReplayCache::default();

    loop {
        if let Some(limit) = max_replies {
            if stats.replies_sent >= limit {
                return Ok(stats);
            }
        }

        match socket.recv_from(&mut recv_buf) {
            Ok((read, peer_addr)) => {
                match decode_datagram_with_options(
                    &recv_buf[..read],
                    config.auth_token.as_deref(),
                    config.allow_legacy_crc32,
                    Some(&mut replay_cache),
                ) {
                    Ok(incoming)
                        if matches!(incoming.kind, FrameKind::Join | FrameKind::Discovery) =>
                    {
                        if socket.send_to(&reply_bytes, peer_addr).is_ok() {
                            stats.replies_sent += 1;
                        }
                    }
                    Ok(_) => {
                        stats.drops.unsupported_kind += 1;
                    }
                    Err(DatagramDecodeError::AuthTagMismatch) => {
                        stats.drops.auth_mismatch += 1;
                    }
                    Err(_) => {
                        stats.drops.malformed += 1;
                    }
                }
            }
            Err(err)
                if matches!(
                    err.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) => {}
            Err(err) => {
                return Err(format!(
                    "failed while receiving UDP discovery request: {err}"
                ));
            }
        }
    }
}

fn encode_datagram(frame: &DiscoveryFrame, auth_token: Option<&str>) -> Vec<u8> {
    let frame_bytes = frame.encode();
    if let Some(token) = auth_token {
        let mut datagram = frame_bytes;
        let timestamp_secs = current_unix_timestamp_secs();
        let nonce = random_nonce();
        let tag = auth_tag_v2(&datagram, token, timestamp_secs, nonce);
        datagram.extend_from_slice(&DISCOVERY_AUTH_MARKER);
        datagram.push(DISCOVERY_AUTH_PROTOCOL_VERSION);
        datagram.extend_from_slice(&timestamp_secs.to_le_bytes());
        datagram.extend_from_slice(&nonce.to_le_bytes());
        datagram.extend_from_slice(&tag);
        datagram
    } else {
        frame_bytes
    }
}

#[cfg(test)]
fn decode_datagram(
    datagram: &[u8],
    auth_token: Option<&str>,
) -> Result<DiscoveryFrame, DatagramDecodeError> {
    decode_datagram_with_options(datagram, auth_token, false, None)
}

fn decode_datagram_with_options(
    datagram: &[u8],
    auth_token: Option<&str>,
    allow_legacy_crc32: bool,
    replay_cache: Option<&mut ReplayCache>,
) -> Result<DiscoveryFrame, DatagramDecodeError> {
    if let Some(token) = auth_token {
        if datagram.len() < DISCOVERY_AUTH_TRAILER_LEN {
            return Err(DatagramDecodeError::DatagramTooShortForAuth);
        }

        let v2_result = decode_v2_auth_datagram(datagram, token, replay_cache);
        if v2_result.is_ok() {
            return v2_result;
        }

        if allow_legacy_crc32 {
            return decode_legacy_crc32_datagram(datagram, token);
        }

        v2_result
    } else {
        DiscoveryFrame::decode(datagram).map_err(DatagramDecodeError::FrameDecode)
    }
}

fn decode_v2_auth_datagram(
    datagram: &[u8],
    token: &str,
    replay_cache: Option<&mut ReplayCache>,
) -> Result<DiscoveryFrame, DatagramDecodeError> {
    let (frame_bytes, trailer) = datagram.split_at(datagram.len() - DISCOVERY_AUTH_TRAILER_LEN);

    let marker = trailer
        .get(0..2)
        .ok_or(DatagramDecodeError::InvalidAuthTrailerLength)?;
    if marker != DISCOVERY_AUTH_MARKER {
        return Err(DatagramDecodeError::InvalidAuthTrailerLength);
    }

    let version = *trailer
        .get(2)
        .ok_or(DatagramDecodeError::InvalidAuthTrailerLength)?;
    if version != DISCOVERY_AUTH_PROTOCOL_VERSION {
        return Err(DatagramDecodeError::UnsupportedAuthProtocolVersion(version));
    }

    let timestamp_bytes: [u8; 8] = trailer
        .get(3..11)
        .ok_or(DatagramDecodeError::InvalidAuthTrailerLength)?
        .try_into()
        .map_err(|_| DatagramDecodeError::InvalidAuthTrailerLength)?;
    let timestamp_secs = u64::from_le_bytes(timestamp_bytes);
    if !timestamp_in_window(timestamp_secs) {
        return Err(DatagramDecodeError::AuthTimestampOutsideWindow);
    }

    let nonce_bytes: [u8; 16] = trailer
        .get(11..27)
        .ok_or(DatagramDecodeError::InvalidAuthTrailerLength)?
        .try_into()
        .map_err(|_| DatagramDecodeError::InvalidAuthTrailerLength)?;
    let nonce = u128::from_le_bytes(nonce_bytes);
    if let Some(cache) = replay_cache {
        if !cache.check_and_insert(nonce) {
            return Err(DatagramDecodeError::ReplayNonceDetected);
        }
    }

    let received_tag = trailer
        .get(27..)
        .ok_or(DatagramDecodeError::InvalidAuthTrailerLength)?;
    let expected_tag = auth_tag_v2(frame_bytes, token, timestamp_secs, nonce);
    if received_tag != expected_tag.as_slice() {
        return Err(DatagramDecodeError::AuthTagMismatch);
    }

    DiscoveryFrame::decode(frame_bytes).map_err(DatagramDecodeError::FrameDecode)
}

fn decode_legacy_crc32_datagram(
    datagram: &[u8],
    token: &str,
) -> Result<DiscoveryFrame, DatagramDecodeError> {
    if datagram.len() < 12 {
        return Err(DatagramDecodeError::DatagramTooShortForAuth);
    }

    let (frame_bytes, tag_bytes) = datagram.split_at(datagram.len() - 4);
    let expected = legacy_auth_tag(frame_bytes, token);
    let received = u32::from_le_bytes(
        tag_bytes
            .try_into()
            .map_err(|_| DatagramDecodeError::InvalidAuthTrailerLength)?,
    );
    if expected != received {
        return Err(DatagramDecodeError::AuthTagMismatch);
    }
    DiscoveryFrame::decode(frame_bytes).map_err(DatagramDecodeError::FrameDecode)
}

fn legacy_auth_tag(frame_bytes: &[u8], token: &str) -> u32 {
    let mut hasher = Hasher::new();
    hasher.update(frame_bytes);
    hasher.update(token.as_bytes());
    hasher.finalize()
}

fn auth_tag_v2(frame_bytes: &[u8], token: &str, timestamp_secs: u64, nonce: u128) -> [u8; 32] {
    let mut mac = HmacSha256::new_from_slice(token.as_bytes())
        .expect("HMAC key setup for discovery auth failed");
    mac.update(frame_bytes);
    mac.update(&DISCOVERY_AUTH_MARKER);
    mac.update(&[DISCOVERY_AUTH_PROTOCOL_VERSION]);
    mac.update(&timestamp_secs.to_le_bytes());
    mac.update(&nonce.to_le_bytes());
    mac.finalize().into_bytes().into()
}

fn current_unix_timestamp_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn timestamp_in_window(timestamp_secs: u64) -> bool {
    let now = current_unix_timestamp_secs();
    if now >= timestamp_secs {
        now - timestamp_secs <= DISCOVERY_AUTH_MAX_SKEW_SECS
    } else {
        timestamp_secs - now <= DISCOVERY_AUTH_MAX_SKEW_SECS
    }
}

fn random_nonce() -> u128 {
    let mut bytes = [0_u8; 16];
    OsRng.fill_bytes(&mut bytes);
    u128::from_le_bytes(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{FrameKind, NodeResources};
    use std::thread;

    #[test]
    fn broadcast_fallback_can_send_without_responses() {
        let frame = DiscoveryFrame {
            kind: FrameKind::Join,
            node: NodeResources::new("node-a", 24.0, 64.0, "8.9", None),
        };

        let config = UdpDiscoveryConfig {
            response_timeout: Duration::from_millis(10),
            ..UdpDiscoveryConfig::default()
        };

        let result = broadcast_and_collect(&frame, &config);
        assert!(result.is_ok());
    }

    #[test]
    fn datagram_auth_validation_rejects_modified_data() {
        let frame = DiscoveryFrame {
            kind: FrameKind::Join,
            node: NodeResources::new("node-b", 12.0, 32.0, "8.6", None),
        };

        let mut packet = encode_datagram(&frame, Some("secret"));
        packet[3] ^= 0x1;

        let result = decode_datagram(&packet, Some("secret"));
        assert!(result.is_err());
    }

    #[test]
    fn responder_replies_to_join_request() {
        let listener = UdpSocket::bind("127.0.0.1:0").expect("bind temp listener");
        let port = listener.local_addr().expect("local addr").port();
        drop(listener);

        let responder_config = UdpDiscoveryConfig {
            bind_addr: SocketAddr::from(([127, 0, 0, 1], port)),
            broadcast_addr: SocketAddr::from(([127, 0, 0, 1], port)),
            response_timeout: Duration::from_millis(400),
            auth_token: Some("token".to_string()),
            ..UdpDiscoveryConfig::default()
        };

        let sender_config = UdpDiscoveryConfig {
            bind_addr: SocketAddr::from(([127, 0, 0, 1], 0)),
            broadcast_addr: SocketAddr::from(([127, 0, 0, 1], port)),
            response_timeout: Duration::from_millis(400),
            auth_token: Some("token".to_string()),
            ..UdpDiscoveryConfig::default()
        };

        let responder_cfg = responder_config.clone();
        let handle = thread::spawn(move || {
            let node = NodeResources::new("node-listener", 16.0, 32.0, "8.6", None);
            respond_once(&node, &responder_cfg)
        });

        thread::sleep(Duration::from_millis(50));

        let join = DiscoveryFrame {
            kind: FrameKind::Join,
            node: NodeResources::new("node-sender", 12.0, 32.0, "8.0", None),
        };
        let replies =
            broadcast_and_collect(&join, &sender_config).expect("broadcast should succeed");
        assert!(!replies.is_empty());
        assert!(replies
            .iter()
            .any(|reply| reply.node.id == "node-listener" && reply.kind == FrameKind::Discovery));

        let responded = handle
            .join()
            .expect("responder thread join")
            .expect("responder should return result");
        assert!(responded.is_some());
    }

    #[test]
    fn respond_once_ignores_auth_mismatch_then_accepts_valid_request() {
        let listener = UdpSocket::bind("127.0.0.1:0").expect("bind temp listener");
        let port = listener.local_addr().expect("local addr").port();
        drop(listener);

        let responder_config = UdpDiscoveryConfig {
            bind_addr: SocketAddr::from(([127, 0, 0, 1], port)),
            response_timeout: Duration::from_millis(700),
            auth_token: Some("secret".to_string()),
            ..UdpDiscoveryConfig::default()
        };

        let responder_cfg = responder_config.clone();
        let handle = thread::spawn(move || {
            let node = NodeResources::new("node-listener", 16.0, 32.0, "8.6", None);
            respond_once(&node, &responder_cfg)
        });

        thread::sleep(Duration::from_millis(50));

        let mismatch_sender = UdpDiscoveryConfig {
            bind_addr: SocketAddr::from(([127, 0, 0, 1], 0)),
            broadcast_addr: SocketAddr::from(([127, 0, 0, 1], port)),
            response_timeout: Duration::from_millis(100),
            auth_token: Some("wrong".to_string()),
            ..UdpDiscoveryConfig::default()
        };
        let valid_sender = UdpDiscoveryConfig {
            bind_addr: SocketAddr::from(([127, 0, 0, 1], 0)),
            broadcast_addr: SocketAddr::from(([127, 0, 0, 1], port)),
            response_timeout: Duration::from_millis(250),
            auth_token: Some("secret".to_string()),
            ..UdpDiscoveryConfig::default()
        };

        let join = DiscoveryFrame {
            kind: FrameKind::Join,
            node: NodeResources::new("node-sender", 12.0, 32.0, "8.0", None),
        };

        let mismatch_replies =
            broadcast_and_collect(&join, &mismatch_sender).expect("mismatch send should succeed");
        assert!(mismatch_replies.is_empty());

        let valid_replies =
            broadcast_and_collect(&join, &valid_sender).expect("valid send should succeed");
        assert!(valid_replies
            .iter()
            .any(|reply| reply.node.id == "node-listener"));

        let responded = handle
            .join()
            .expect("responder thread join")
            .expect("responder should return result");
        assert!(responded.is_some());
    }

    #[test]
    fn broadcast_collects_multiple_replies_in_single_window() {
        let listener = UdpSocket::bind("127.0.0.1:0").expect("bind temp listener");
        let port = listener.local_addr().expect("local addr").port();
        drop(listener);

        let responder = thread::spawn(move || {
            let socket = UdpSocket::bind(SocketAddr::from(([127, 0, 0, 1], port)))
                .expect("bind burst responder");
            socket
                .set_read_timeout(Some(Duration::from_millis(700)))
                .expect("set timeout");

            let mut recv_buf = vec![0_u8; 1024];
            let (read, peer_addr) = socket.recv_from(&mut recv_buf).expect("recv request");
            let incoming = decode_datagram(&recv_buf[..read], Some("secret"))
                .expect("decode incoming datagram");
            assert_eq!(incoming.kind, FrameKind::Join);

            for node_id in ["node-a", "node-b"] {
                let reply = DiscoveryFrame {
                    kind: FrameKind::Discovery,
                    node: NodeResources::new(node_id, 16.0, 32.0, "8.6", None),
                };
                let packet = encode_datagram(&reply, Some("secret"));
                socket
                    .send_to(&packet, peer_addr)
                    .expect("send reply datagram");
            }
        });

        thread::sleep(Duration::from_millis(50));

        let sender = UdpDiscoveryConfig {
            bind_addr: SocketAddr::from(([127, 0, 0, 1], 0)),
            broadcast_addr: SocketAddr::from(([127, 0, 0, 1], port)),
            response_timeout: Duration::from_millis(250),
            auth_token: Some("secret".to_string()),
            ..UdpDiscoveryConfig::default()
        };
        let join = DiscoveryFrame {
            kind: FrameKind::Join,
            node: NodeResources::new("node-sender", 12.0, 24.0, "8.0", None),
        };

        let replies = broadcast_and_collect(&join, &sender).expect("broadcast should succeed");
        assert!(replies.iter().any(|reply| reply.node.id == "node-a"));
        assert!(replies.iter().any(|reply| reply.node.id == "node-b"));

        responder.join().expect("responder join");
    }

    #[test]
    fn serve_discovery_with_stats_tracks_drop_reasons() {
        let listener = UdpSocket::bind("127.0.0.1:0").expect("bind temp listener");
        let port = listener.local_addr().expect("local addr").port();
        drop(listener);

        let responder_config = UdpDiscoveryConfig {
            bind_addr: SocketAddr::from(([127, 0, 0, 1], port)),
            response_timeout: Duration::from_millis(1500),
            auth_token: Some("secret".to_string()),
            ..UdpDiscoveryConfig::default()
        };

        let responder_cfg = responder_config.clone();
        let handle = thread::spawn(move || {
            let node = NodeResources::new("node-listener", 16.0, 32.0, "8.6", None);
            serve_discovery_with_stats(&node, &responder_cfg, Some(1))
        });

        thread::sleep(Duration::from_millis(50));

        let raw_sender =
            UdpSocket::bind(SocketAddr::from(([127, 0, 0, 1], 0))).expect("bind raw sender");
        raw_sender
            .send_to(
                &[0xaa, 0xbb, 0xcc],
                SocketAddr::from(([127, 0, 0, 1], port)),
            )
            .expect("send malformed datagram");

        let mismatch_sender = UdpDiscoveryConfig {
            bind_addr: SocketAddr::from(([127, 0, 0, 1], 0)),
            broadcast_addr: SocketAddr::from(([127, 0, 0, 1], port)),
            response_timeout: Duration::from_millis(120),
            auth_token: Some("wrong".to_string()),
            ..UdpDiscoveryConfig::default()
        };
        let valid_sender = UdpDiscoveryConfig {
            bind_addr: SocketAddr::from(([127, 0, 0, 1], 0)),
            broadcast_addr: SocketAddr::from(([127, 0, 0, 1], port)),
            response_timeout: Duration::from_millis(200),
            auth_token: Some("secret".to_string()),
            ..UdpDiscoveryConfig::default()
        };

        let mismatch_join = DiscoveryFrame {
            kind: FrameKind::Join,
            node: NodeResources::new("node-mismatch", 8.0, 16.0, "8.0", None),
        };
        let unsupported = DiscoveryFrame {
            kind: FrameKind::Attestation,
            node: NodeResources::new("node-unsupported", 8.0, 16.0, "8.0", None),
        };
        let valid_join = DiscoveryFrame {
            kind: FrameKind::Join,
            node: NodeResources::new("node-valid", 8.0, 16.0, "8.0", None),
        };

        let mismatch_replies =
            broadcast_and_collect(&mismatch_join, &mismatch_sender).expect("send mismatch join");
        assert!(mismatch_replies.is_empty());

        let unsupported_replies =
            broadcast_and_collect(&unsupported, &valid_sender).expect("send unsupported frame");
        assert!(unsupported_replies.is_empty());

        let valid_replies = broadcast_and_collect(&valid_join, &valid_sender).expect("send join");
        assert!(!valid_replies.is_empty());

        let stats = handle
            .join()
            .expect("responder thread join")
            .expect("responder should return result");

        assert_eq!(stats.replies_sent, 1);
        assert!(stats.drops.malformed >= 1);
        assert!(stats.drops.auth_mismatch >= 1);
        assert!(stats.drops.unsupported_kind >= 1);
    }

    #[test]
    fn respond_once_survives_mixed_traffic_soak_before_valid_join() {
        let listener = UdpSocket::bind("127.0.0.1:0").expect("bind temp listener");
        let port = listener.local_addr().expect("local addr").port();
        drop(listener);

        let responder_config = UdpDiscoveryConfig {
            bind_addr: SocketAddr::from(([127, 0, 0, 1], port)),
            response_timeout: Duration::from_millis(2600),
            auth_token: Some("secret".to_string()),
            ..UdpDiscoveryConfig::default()
        };

        let responder_cfg = responder_config.clone();
        let handle = thread::spawn(move || {
            let node = NodeResources::new("node-listener", 16.0, 32.0, "8.6", None);
            respond_once(&node, &responder_cfg)
        });

        thread::sleep(Duration::from_millis(50));

        let raw_sender =
            UdpSocket::bind(SocketAddr::from(([127, 0, 0, 1], 0))).expect("bind raw sender");
        for _ in 0..24 {
            raw_sender
                .send_to(&[0xde, 0xad], SocketAddr::from(([127, 0, 0, 1], port)))
                .expect("send malformed datagram");
        }

        let wrong_sender = UdpDiscoveryConfig {
            bind_addr: SocketAddr::from(([127, 0, 0, 1], 0)),
            broadcast_addr: SocketAddr::from(([127, 0, 0, 1], port)),
            response_timeout: Duration::from_millis(45),
            auth_token: Some("wrong".to_string()),
            ..UdpDiscoveryConfig::default()
        };
        let valid_sender = UdpDiscoveryConfig {
            bind_addr: SocketAddr::from(([127, 0, 0, 1], 0)),
            broadcast_addr: SocketAddr::from(([127, 0, 0, 1], port)),
            response_timeout: Duration::from_millis(260),
            auth_token: Some("secret".to_string()),
            ..UdpDiscoveryConfig::default()
        };

        let wrong_join = DiscoveryFrame {
            kind: FrameKind::Join,
            node: NodeResources::new("node-wrong", 8.0, 16.0, "8.0", None),
        };
        for _ in 0..4 {
            let replies =
                broadcast_and_collect(&wrong_join, &wrong_sender).expect("send wrong join frame");
            assert!(replies.is_empty());
        }

        let valid_join = DiscoveryFrame {
            kind: FrameKind::Join,
            node: NodeResources::new("node-valid", 8.0, 16.0, "8.0", None),
        };
        let valid_replies =
            broadcast_and_collect(&valid_join, &valid_sender).expect("send valid join frame");
        assert!(valid_replies
            .iter()
            .any(|reply| reply.node.id == "node-listener"));

        let responded = handle
            .join()
            .expect("responder thread join")
            .expect("responder should return result");
        assert!(responded.is_some());
    }
}
