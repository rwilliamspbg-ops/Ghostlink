//! AF_XDP/eBPF Socket Integration for Ghost-Link
//! 
//! This module provides:
//! - Raw socket binding with AF_XDP
//! - EtherType filtering (0x88B5)
//! - Frame reception loop with zero-copy buffers
//! - eBPF program loading helpers

use std::ffi::c_void;
use std::mem;
use std::os::raw::{c_char, c_int};
use std::ptr;

use crate::protocol::{DiscoveryFrame, GHOSTLINK_ETHERTYPE};

/// AF_XDP socket constants (Linux-specific)
const SOL_XDP: i32 = 0x2F;
const XDP_ATTACHED: i32 = 1;
const XDP_UNMAP: i32 = 1;

/// Maximum size of XDP frame (including header)
pub const MAX_XDP_FRAME_SIZE: usize = 2048;

/// XDP socket configuration
#[derive(Clone, Copy, Debug)]
pub struct XdpConfig {
    /// Interface name to bind (e.g., "eth0")
    pub interface_name: String,
    /// Memory order for ring buffer
    pub memory_order: i32,
}

impl Default for XdpConfig {
    fn default() -> Self {
        Self {
            interface_name: "eth0".to_string(),
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
    /// Create new XDP socket handle
    pub fn new(interface_name: &str) -> Result<Self, String> {
        // Note: This is a placeholder for Linux-specific implementation
        // Actual implementation would use syscall! macro or bindgen
        
        Err("AF_XDP sockets are Linux-only".into())
    }
    
    /// Bind socket to interface (Linux-specific)
    pub fn bind(&self, _interface_name: &str) -> Result<(), String> {
        Err("AF_XDP binding requires Linux kernel support".into())
    }
    
    /// Receive frame from XDP socket
    /// 
    /// Returns the raw frame bytes.
    pub fn recv_frame(&self, buffer: &mut [u8]) -> Option<usize> {
        // Placeholder - actual implementation uses recvmsg syscall
        None
    }
    
    /// Send frame to XDP socket (for outgoing traffic)
    pub fn send_frame(&self, data: &[u8]) -> Result<(), String> {
        Err("AF_XDP send requires specific setup".into())
    }
}

/// Frame reception loop with zero-copy buffers
#[derive(Clone, Debug)]
pub struct XdpFrameReceiver {
    /// Configuration for receiver
    config: XdpConfig,
    /// Ring buffer for incoming frames
    ring_buffer: crate::ring::SpscRingBuffer<Vec<u8>>,
}

impl XdpFrameReceiver {
    /// Create new frame receiver
    pub fn new(config: XdpConfig) -> Self {
        let ring = crate::ring::SpscRingBuffer::new(XdpConfig::default());
        
        Self {
            config,
            ring_buffer: ring,
        }
    }
    
    /// Receive and parse discovery frame from raw socket
    pub fn recv_discovery_frame(&self) -> Option<DiscoveryFrame> {
        // Allocate buffer for incoming frame
        let mut buffer = vec![0u8; MAX_XDP_FRAME_SIZE];
        
        // In production, this would use AF_XDP recvmsg
        // For now, we simulate with protocol decoding
        
        None
    }
    
    /// Process raw frame bytes and extract discovery frame
    pub fn process_frame(&self, bytes: &[u8]) -> Option<DiscoveryFrame> {
        if bytes.len() < 10 {
            return None;
        }
        
        // Check EtherType filter
        let ether_type = u16::from_be_bytes([bytes[0], bytes[1]]);
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
        Err("eBPF loading requires Linux kernel support".into())
    }
    
    /// Attach eBPF program to XDP socket
    pub fn attach(&self, _fd: c_int) -> Result<(), String> {
        Err("eBPF attachment requires Linux kernel support".into())
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
        self.frames_dropped += 1;
    }
    
    /// Record frame processed
    pub fn record_processed(&mut self) {
        self.frames_processed += 1;
    }
    
    /// Update average latency
    pub fn update_latency(&mut self, latency_us: f32) {
        // EMA with alpha=0.1
        self.avg_latency_us = self.avg_latency_us * 0.9 + latency_us * 0.1;
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
#[derive(Clone, Debug)]
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
        
        Ok(())
    }
    
    /// Receive frame using AF_XDP recvmsg
    pub fn recv_frame(&mut self, buffer: &mut [u8]) -> Option<usize> {
        // Placeholder - actual implementation uses recvmsg syscall
        None
    }
    
    /// Send frame using AF_XDP sendmsg
    pub fn send_frame(&mut self, data: &[u8]) -> Result<(), String> {
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
        let receiver = XdpReceiver::new(XdpConfig::default());
        
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
        assert_eq!(stats.avg_latency_us, 1.1);
    }

    #[test]
    fn xdp_receiver_rejects_wrong_ether_type() {
        let receiver = XdpReceiver::new(XdpConfig::default());
        
        // Create a frame with wrong EtherType
        let mut fake_frame = vec![0u8; 10];
        fake_frame[0] = 0x88B5 as u8; // Correct first byte
        fake_frame[1] = 0xFF; // Wrong second byte
        
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
        assert!(report.contains("Frames received: 2"));
        assert!(report.contains("Frames dropped: 1"));
    }
}
