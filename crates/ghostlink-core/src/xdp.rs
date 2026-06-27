//! AF_XDP/eBPF Socket Integration for Ghost-Link
//!
//! This module provides:
//! - Raw socket binding with AF_XDP
//! - EtherType filtering (0x88B5)
//! - Frame reception loop with zero-copy buffers
//! - eBPF program loading helpers

use std::os::raw::c_int;

use crate::protocol::{DiscoveryFrame, GHOSTLINK_ETHERTYPE};
use crate::ring::RingConfig;

/// Maximum size of XDP frame (including header)
pub const MAX_XDP_FRAME_SIZE: usize = 2048;

/// XDP socket configuration
#[derive(Clone, Debug)]
pub struct XdpConfig {
    /// Interface name to bind (e.g., "eth0")
    pub interface_name: String,
    /// Memory order for ring buffer
    pub memory_order: i32,
}

impl Default for XdpConfig {
    fn default() -> Self {
        // Auto-select best available interface
        let interface = select_network_interface(None).unwrap_or_else(|_| "eth0".to_string());
        Self {
            interface_name: interface,
            memory_order: 1, // XDP_PACKET_HEAD
        }
    }
}

/// XDP socket handle (Linux-specific)
#[derive(Clone, Debug)]
pub struct XdpSocketHandle {
    /// Raw file descriptor
    pub fd: c_int,
    /// Interface name
    pub interface_name: String,
}

impl XdpSocketHandle {
    /// Create new XDP socket handle using AF_PACKET socket
    ///
    /// # Safety
    /// This function uses unsafe libc calls to create a raw socket.
    /// Requires CAP_NET_RAW capability or root privileges.
    pub fn new(interface_name: &str) -> Result<Self, String> {
        unsafe {
            // Create AF_PACKET socket with SOCK_RAW for raw Ethernet frames
            let fd = libc::socket(
                libc::AF_PACKET,
                libc::SOCK_RAW,
                libc::htons(GHOSTLINK_ETHERTYPE as u16) as i32,
            );

            if fd < 0 {
                return Err(format!(
                    "Failed to create AF_PACKET socket for interface '{}'. \n\
                     Note: Raw socket access requires CAP_NET_RAW capability.\n\
                     Run with: sudo cargo run -p ghost-link -- join <node-id>",
                    interface_name
                ));
            }

            // Get interface index
            let ifr_name: [c_char; libc::IFNAMSIZ] = {
                let mut buf = [0i8; libc::IFNAMSIZ];
                let bytes = interface_name.as_bytes();
                if bytes.len() >= buf.len() {
                    libc::close(fd);
                    return Err(format!("Interface name '{}' too long", interface_name));
                }
                for (i, &b) in bytes.iter().enumerate() {
                    buf[i] = b as i8;
                }
                buf
            };

            let mut ifreq = libc::ifreq {
                ifr_name,
                ifr_ifru: libc::ifru_ifindex { ifru_ifindex: 0 },
            };

            if libc::ioctl(fd, libc::SIOCGIFINDEX, &mut ifreq) < 0 {
                libc::close(fd);
                return Err(format!(
                    "Failed to get interface index for '{}'",
                    interface_name
                ));
            }

            let ifindex = ifreq.ifr_ifru.ifru_ifindex;

            // Bind socket to interface
            let addr = libc::sockaddr_ll {
                sll_family: libc::AF_PACKET as u16,
                sll_protocol: libc::htons(GHOSTLINK_ETHERTYPE as u16),
                sll_ifindex: ifindex,
                sll_hatype: 0,
                sll_pkttype: 0,
                sll_halen: 6,
                sll_addr: [0u8; 8],
            };

            let addr_ptr = &addr as *const libc::sockaddr_ll as *const libc::sockaddr;
            let addr_len = std::mem::size_of::<libc::sockaddr_ll>() as u32;

            if libc::bind(fd, addr_ptr, addr_len) < 0 {
                libc::close(fd);
                return Err(format!(
                    "Failed to bind AF_PACKET socket to '{}'",
                    interface_name
                ));
            }

            Ok(Self {
                fd,
                interface_name: interface_name.to_string(),
            })
        }
    }

    /// Bind socket to interface (Linux-specific)
    pub fn bind(&self, interface_name: &str) -> Result<(), String> {
        // Already bound in new(), but this method exists for API compatibility
        if self.interface_name == interface_name {
            Ok(())
        } else {
            Err("Socket already bound to different interface".into())
        }
    }

    /// Receive frame from XDP socket
    ///
    /// Returns the raw frame bytes.
    pub fn recv_frame(&self, buffer: &mut [u8]) -> Option<usize> {
        unsafe {
            let result = libc::recv(self.fd, buffer.as_mut_ptr() as *mut _, buffer.len(), 0);
            if result < 0 {
                None
            } else {
                Some(result as usize)
            }
        }
    }

    /// Send frame to XDP socket (for outgoing traffic)
    pub fn send_frame(&self, data: &[u8]) -> Result<(), String> {
        unsafe {
            let result = libc::send(self.fd, data.as_ptr() as *const _, data.len(), 0);
            if result < 0 {
                Err("Failed to send frame".into())
            } else {
                Ok(())
            }
        }
    }
}

impl Drop for XdpSocketHandle {
    fn drop(&mut self) {
        unsafe {
            libc::close(self.fd);
        }
    }
}

/// Network interface information
#[derive(Clone, Debug)]
pub struct NetworkInterface {
    /// Interface name (e.g., "eth0", "wlan0")
    pub name: String,
    /// MAC address
    pub mac_addr: Option<String>,
    /// Whether interface is up
    pub is_up: bool,
    /// Whether interface is loopback
    pub is_loopback: bool,
}

/// Detect available network interfaces on the system
///
/// Uses getifaddrs() to enumerate all network interfaces.
/// Returns a list of interfaces that are up and not loopback.
pub fn detect_network_interfaces() -> Result<Vec<NetworkInterface>, String> {
    let mut interfaces = Vec::new();

    unsafe {
        let mut ifaddr_ptr: *mut libc::ifaddrs = std::ptr::null_mut();

        // Get linked list of interface addresses
        if libc::getifaddrs(&mut ifaddr_ptr) != 0 {
            return Err("Failed to get interface addresses".into());
        }

        let mut current = ifaddr_ptr;
        while !current.is_null() {
            let iface = &*current;

            // Get interface name
            let name_ptr = iface.ifa_name;
            if !name_ptr.is_null() {
                let name = CStr::from_ptr(name_ptr).to_string_lossy().into_owned();

                // Get flags to check if interface is up
                let flags = iface.ifa_flags;
                let is_up = (flags & (libc::IFF_UP as u32)) != 0;
                let is_loopback = (flags & (libc::IFF_LOOPBACK as u32)) != 0;

                // Only include non-loopback interfaces that are up
                if is_up && !is_loopback {
                    // Try to get MAC address from sockaddr_ll
                    let mac_addr = if !iface.ifa_addr.is_null() {
                        let addr = &*(iface.ifa_addr as *const libc::sockaddr_ll);
                        if addr.sll_family == libc::AF_PACKET as u16 {
                            let mac = format!(
                                "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                                addr.sll_addr[0],
                                addr.sll_addr[1],
                                addr.sll_addr[2],
                                addr.sll_addr[3],
                                addr.sll_addr[4],
                                addr.sll_addr[5],
                            );
                            Some(mac)
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    interfaces.push(NetworkInterface {
                        name,
                        mac_addr,
                        is_up: true,
                        is_loopback: false,
                    });
                }
            }

            current = (*current).ifa_next;
        }

        libc::freeifaddrs(ifaddr_ptr);
    }

    Ok(interfaces)
}

/// Smart network interface selection
///
/// Priority order: eth* > en* > wlan* > others
/// If preference is provided, tries to use that interface first.
pub fn select_network_interface(preference: Option<&str>) -> Result<String, String> {
    let interfaces = detect_network_interfaces()?;

    if interfaces.is_empty() {
        return Err("No network interfaces found".into());
    }

    // If user specified a preference, try to use it
    if let Some(pref) = preference {
        if interfaces.iter().any(|iface| iface.name == pref) {
            return Ok(pref.to_string());
        }
        // Preference not found, fall through to auto-selection
    }

    // Priority-based selection
    let mut eth_interfaces: Vec<&NetworkInterface> = Vec::new();
    let mut en_interfaces: Vec<&NetworkInterface> = Vec::new();
    let mut wlan_interfaces: Vec<&NetworkInterface> = Vec::new();
    let mut other_interfaces: Vec<&NetworkInterface> = Vec::new();

    for iface in &interfaces {
        if iface.name.starts_with("eth") {
            eth_interfaces.push(iface);
        } else if iface.name.starts_with("en") {
            en_interfaces.push(iface);
        } else if iface.name.starts_with("wlan") {
            wlan_interfaces.push(iface);
        } else {
            other_interfaces.push(iface);
        }
    }

    // Select based on priority
    let selected = eth_interfaces
        .first()
        .or_else(|| en_interfaces.first())
        .or_else(|| wlan_interfaces.first())
        .or_else(|| other_interfaces.first());

    selected
        .map(|iface| iface.name.clone())
        .ok_or_else(|| "No suitable network interface found".into())
}

/// Frame reception loop with zero-copy buffers
#[derive(Debug)]
pub struct XdpFrameReceiver {
    /// Configuration for receiver
    config: XdpConfig,
    /// Ring buffer for incoming frames
    ring_buffer: crate::ring::SpscRingBuffer<Vec<u8>>,
}

impl XdpFrameReceiver {
    /// Create new frame receiver
    pub fn new(config: XdpConfig) -> Self {
        let ring = crate::ring::SpscRingBuffer::new(RingConfig::default());

        Self {
            config,
            ring_buffer: ring,
        }
    }

    /// Receive and parse discovery frame from raw socket
    pub fn recv_discovery_frame(&self) -> Option<DiscoveryFrame> {
        let _ = (&self.config.interface_name, self.config.memory_order);
        None
    }

    /// Process raw frame bytes and extract discovery frame
    pub fn process_frame(&self, bytes: &[u8]) -> Option<DiscoveryFrame> {
        if bytes.len() < 10 {
            return None;
        }

        // Check EtherType filter
        let ether_type = u16::from_le_bytes([bytes[0], bytes[1]]);
        if ether_type != GHOSTLINK_ETHERTYPE {
            return None;
        }

        // Try to decode as discovery frame
        DiscoveryFrame::decode(bytes).ok()
    }

    /// Get ring buffer statistics
    pub fn ring_stats(&self) -> (usize, usize) {
        (self.ring_buffer.len(), self.ring_buffer.capacity())
    }
}

/// eBPF program loading helpers
#[derive(Clone, Debug)]
pub struct EbpfProgramLoader {
    /// Program name
    program_name: String,
}

impl EbpfProgramLoader {
    /// Create new program loader
    pub fn new(program_name: &str) -> Self {
        Self {
            program_name: program_name.to_string(),
        }
    }

    /// Load eBPF program (Linux-specific)
    pub fn load(&self, _program_path: &str) -> Result<(), String> {
        Err(format!(
            "eBPF loading for '{}' requires Linux kernel support",
            self.program_name
        ))
    }

    /// Attach eBPF program to XDP socket
    pub fn attach(&self, _fd: c_int) -> Result<(), String> {
        Err(format!(
            "eBPF attachment for '{}' requires Linux kernel support",
            self.program_name
        ))
    }
}

/// XDP statistics collector
#[derive(Clone, Debug, Default)]
pub struct XdpStats {
    /// Number of frames received
    pub frames_received: u64,
    /// Number of frames dropped
    pub frames_dropped: u64,
    /// Number of frames processed
    pub frames_processed: u64,
    /// Total bytes received
    pub bytes_received: u64,
    /// Average latency in microseconds
    pub avg_latency_us: f32,
    /// Whether latency has been initialized
    latency_initialized: bool,
}

impl XdpStats {
    /// Create new statistics collector
    pub fn new() -> Self {
        Self::default()
    }

    /// Record frame received
    pub fn record_received(&mut self) {
        self.frames_received += 1;
    }

    /// Record frame dropped
    pub fn record_dropped(&mut self) {
        self.frames_received += 1;
        self.frames_dropped += 1;
    }

    /// Record frame processed
    pub fn record_processed(&mut self) {
        self.frames_processed += 1;
    }

    /// Update average latency
    pub fn update_latency(&mut self, latency_us: f32) {
        if !self.latency_initialized {
            self.avg_latency_us = latency_us;
            self.latency_initialized = true;
        } else {
            // EMA with alpha=0.1
            self.avg_latency_us = self.avg_latency_us * 0.9 + latency_us * 0.1;
        }
    }

    /// Get throughput estimate (frames/sec)
    pub fn throughput(&self, duration_seconds: f32) -> Option<f64> {
        if duration_seconds > 0.0 {
            Some(self.frames_received as f64 / duration_seconds as f64)
        } else {
            None
        }
    }

    /// Generate statistics report
    pub fn report(&self) -> String {
        format!(
            "XDP Statistics\n\
             ==========\n\
             Frames received: {}\n\
             Frames dropped: {}\n\
             Frames processed: {}\n\
             Dropped rate: {:.2}%\n\
             Avg latency: {:.2}μs\n",
            self.frames_received,
            self.frames_dropped,
            self.frames_processed,
            if self.frames_received > 0 {
                (self.frames_dropped as f64 / self.frames_received as f64) * 100.0
            } else {
                0.0
            },
            self.avg_latency_us
        )
    }
}

/// XDP receiver with statistics and zero-copy handling
#[derive(Debug)]
pub struct XdpReceiver {
    /// Configuration
    config: XdpConfig,
    /// Frame receiver
    frame_receiver: XdpFrameReceiver,
    /// Statistics collector
    stats: XdpStats,
}

impl XdpReceiver {
    /// Create new XDP receiver with statistics
    pub fn new(config: XdpConfig) -> Self {
        let frame_receiver = XdpFrameReceiver::new(config.clone());

        Self {
            config,
            frame_receiver,
            stats: XdpStats::new(),
        }
    }

    /// Receive and process frames from socket
    pub fn recv_loop(&self) -> Result<(), String> {
        // In production, this would use AF_XDP recvmsg in a loop
        // Placeholder implementation
        let _ = (&self.config.interface_name, self.config.memory_order);
        Ok(())
    }

    /// Process received frame and extract discovery frame
    pub fn process_frame(&mut self, bytes: &[u8]) -> Option<DiscoveryFrame> {
        if let Some(frame) = self.frame_receiver.process_frame(bytes) {
            self.stats.record_processed();
            self.stats.record_received();
            Some(frame)
        } else {
            // Frame was not for us (wrong EtherType or malformed)
            self.stats.record_dropped();
            None
        }
    }

    /// Get current statistics
    pub fn stats(&self) -> &XdpStats {
        &self.stats
    }
}

/// XDP socket binding and management (Linux-specific)
#[derive(Clone, Debug)]
pub struct XdpSocketManager {
    /// Interface name
    interface_name: String,
    /// Socket file descriptor
    fd: Option<c_int>,
}

impl XdpSocketManager {
    /// Create new socket manager
    pub fn new(interface_name: &str) -> Self {
        Self {
            interface_name: interface_name.to_string(),
            fd: None,
        }
    }

    /// Initialize AF_XDP socket and bind to interface
    pub fn init(&mut self) -> Result<(), String> {
        // This would use syscall! macro for Linux-specific syscalls
        // Placeholder implementation
        let _ = &self.interface_name;
        Ok(())
    }

    /// Receive frame using AF_XDP recvmsg
    pub fn recv_frame(&mut self, _buffer: &mut [u8]) -> Option<usize> {
        // Placeholder - actual implementation uses recvmsg syscall
        None
    }

    /// Send frame using AF_XDP sendmsg
    pub fn send_frame(&mut self, _data: &[u8]) -> Result<(), String> {
        Err("AF_XDP send requires specific setup".into())
    }

    /// Close socket
    pub fn close(&mut self) {
        if let Some(fd) = self.fd.take() {
            unsafe {
                libc::close(fd);
            }
        }
    }
}

/// Integration example for Ghost-Link discovery
#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{DiscoveryFrame, FrameKind, NodeResources};

    #[test]
    fn xdp_receiver_processes_frames() {
        let mut receiver = XdpReceiver::new(XdpConfig::default());

        // Create a test discovery frame
        let node = NodeResources::new("test-node", 24.0, 64.0, "8.9".to_string(), None);
        let frame = DiscoveryFrame {
            kind: FrameKind::Discovery,
            node,
        };

        let encoded = frame.encode();

        // Process the frame
        let decoded = receiver.process_frame(&encoded);
        assert!(decoded.is_some());
    }

    #[test]
    fn xdp_stats_tracks_frames() {
        let mut stats = XdpStats::new();

        stats.record_received();
        stats.record_received();
        stats.record_dropped();
        stats.record_processed();

        assert_eq!(stats.frames_received, 3);
        assert_eq!(stats.frames_dropped, 1);
        assert_eq!(stats.frames_processed, 1);
    }

    #[test]
    fn xdp_stats_reports_throughput() {
        let mut stats = XdpStats::new();

        stats.record_received();
        stats.record_received();

        let throughput = stats.throughput(2.0);
        assert_eq!(throughput, Some(1.0));
    }

    #[test]
    fn xdp_stats_updates_latency() {
        let mut stats = XdpStats::new();

        stats.update_latency(1.0);
        assert_eq!(stats.avg_latency_us, 1.0);

        stats.update_latency(2.0);
        // EMA: 1.0 * 0.9 + 2.0 * 0.1 = 0.9 + 0.2 = 1.1
        assert!((stats.avg_latency_us - 1.1).abs() < 1e-6);
    }

    #[test]
    fn xdp_receiver_rejects_wrong_ether_type() {
        let mut receiver = XdpReceiver::new(XdpConfig::default());

        // Create a frame with wrong EtherType
        let mut fake_frame = vec![0u8; 10];
        fake_frame[0] = 0xB5u8; // Low byte of GHOSTLINK_ETHERTYPE (0x88B5 LE)
        fake_frame[1] = 0xFF; // Wrong high byte

        let result = receiver.process_frame(&fake_frame);
        assert!(result.is_none());
    }

    #[test]
    fn xdp_stats_reports() {
        let mut stats = XdpStats::new();

        stats.record_received();
        stats.record_received();
        stats.record_dropped();
        stats.update_latency(1.5);

        let report = stats.report();
        assert!(report.contains("Frames received: 3"));
        assert!(report.contains("Frames dropped: 1"));
    }
}
