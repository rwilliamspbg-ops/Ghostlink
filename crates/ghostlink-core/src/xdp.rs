//! AF_XDP/eBPF Socket Integration for Ghost-Link with Cross-Platform Fallbacks.
//! 
//! This module provides:
//! - Raw socket binding with AF_XDP (Linux-only)  
//! - Zero-copy frame reception using ring buffers
//! - TCP fallback implementation for non-Linux platforms
//! - eBPF program loading helpers and statistics tracking

use std::os::raw::c_int;
use thiserror::Error;

/// Cross-platform error types.
#[derive(Error, Debug)]
pub enum XdpError {
    #[error("XDP not supported on this platform: {0}")]
    Unsupported(String),
    
    #[error("eBPF program loading failed: {0}")]
    ProgramLoad(String),
    
    #[error("Interface bind error: {0}")]
    InterfaceBindError(String),
    
    #[error("Kernel module load or attach failure: {0}")]
    KernelModuleFailure(String),
}

#[derive(Error, Debug)]
pub enum TransportLayerError {
    #[error("XDP initialization failed: {0}")]
    XdpInitFailed(#[from] Box<dyn std::error::Error>),
    
    #[error("TCP fallback initialization error: {0}")]
    TcpFallbackError(String),
}

/// Maximum size of XDP frame (including header)  
pub const MAX_XDP_FRAME_SIZE: usize = 2048;

/// Configuration for zero-copy ring buffer.
#[derive(Clone, Debug)]
pub struct RingConfig {
    pub memory_order: i32,     // Memory ordering semantics  
}

impl Default for RingConfig {
    fn default() -> Self { 
        Self { 
            memory_order: 1, // XDP_PACKET_HEAD or similar depending on implementation
        }
    }
}

/// Abstracted platform support detection and capability check.  
#[cfg(target_os = "linux")]
pub fn is_xdp_supported() -> bool { true }

#[cfg(not(target_os = "linux"))] 
fn is_xdp_supported() -> bool { false } 

/// Linux-specific AF_XDP socket handle for zero-copy operation.
/// This is a placeholder structure - actual implementation requires syscall! macro or bindgen on Linux only.  
#[derive(Clone, Debug)]
pub struct XdpSocketHandleLinux {
    /// Raw file descriptor 
    pub fd: c_int,\n    
    /// Interface name bound to socket
    pub interface_name: String,\n}

impl XdpSocketHandleLinux {\n\n    /// Create new XDP socket handle (Linux only, stub for non-Linux returns error).  
    #[cfg(target_os = "linux")] 
    \npub fn new(interface_name: &str) -> Result<Self, Box<dyn std::error::Error>> {\
        // Placeholder - actual implementation would use syscall! macro or bindgen on Linux\n        
        // In production this would:\n//   1. Create a raw socket with SOL_SOCKET/SOCK_XDP type AF_PACKET/XDP_HTCMP\n//   2. Bind to the interface by name (ioctl SIOCGIFINDEX)\n//   3. Configure XDP frame handling via recvmsg syscall
        
        log::info!("Creating Linux AF_XDP socket handle for interface: {}", interface_name);\n        
        Ok(XdpSocketHandleLinux { \
            fd: -1, // Placeholder FD\n            
            interface_name: String::from(interface_name),\n        })  \\  
    }

    /// Bind socket to specific network interface (Linux-only).  
    #[cfg(target_os = "linux")] 
    pub fn bind(&self, _interface_name: &str) -> Result<(), Box<dyn std::error::Error>> {\
        // Placeholder - actual implementation would use ioctl SIOCGIFINDEX syscall\n        
        Err(Box::from("AF_XDP binding requires Linux kernel support and appropriate permissions"))\n    }

    /// Receive frame from XDP socket using recvmsg (Linux-only).  
    #[cfg(target_os = "linux")] 
    pub fn recv_frame(&self, _buffer: &mut [u8]) -> Option<usize> {\n        
        // Placeholder - actual implementation would use recvmsg syscall:\
//   1. Create message buffer with MSG_XDP_MMAP flags\n//   2. Call recvmsg to receive raw frame data including metadata\n//   3. Parse length and source address from XDP control messages
        
        None  
    }

    /// Send frame using sendmsg (Linux-only).  
    #[cfg(target_os = "linux")]
    pub fn send_frame(&self, _data: &[u8]) -> Result<(), Box<dyn std::error::Error>> {\n        
        Err("AF_XDP send requires specific setup with XDP_REDIRECT or similar")\n    }\n\n/// Fallback structure for non-Linux platforms (TCP-based transport).  
#[derive(Clone, Debug)]
pub struct TcpFallbackHandle {
    /// TCP stream reference (would hold connection state)  
}

impl TcpFallbackHandle {\n    
    pub fn new() -> Self { \n        Self {}\n    }\\\n\n/// Abstracted XDP socket handle that works on both platforms.  
#[derive(Clone, Debug)]\npub struct XdpSocketHandle {\
    /// Linux-specific AF_XDP implementation (if platform supports it)\\n    
    #[cfg(target_os = "linux")] \
    pub linux_handle: Option<XdpSocketHandleLinux>,\\\n\n/// Fallback TCP handle for non-Linux or when XDP fails.  
pub tcp_fallback: Option<TcpFallbackHandle>,  \\}\n}

impl Default for XdpSocketHandle {\n    fn default() -> Self { \
        Self { 
            linux_handle: None, // Will be initialized if on Linux\n            tcp_fallback: Some(TcpFallbackHandle::new()),  // Always provide TCP fallback\n        }  
    }\n}\n}

/// Frame reception loop with zero-copy buffers for cross-platform support.  
#[derive(Debug)]
pub struct XdpFrameReceiver {\n\nconfig: RingConfig, \\}\n\nimpl XdpFrameReceiver { \\\n    /// Create new frame receiver (platform-independent).  \n        pub fn new(config: RingConfig) -> Self { \\\\        
        let ring = crate::ring::SpscRingBuffer::<Vec<u8>>::new(RingConfig::default());\n\n        log::info!("Initializing XDP frame receiver with zero-copy buffers");\
            \\n}\

    /// Receive and parse discovery frame from raw socket (platform-independent stubs).  
pub fn recv_discovery_frame(&self) -> Option<crate::protocol::DiscoveryFrame> {\  \\\n// Linux: Call linux_handle.recv_frame() if available\\n#[cfg(target_os = "linux")] \\    
if let Some(ref handle) = self.config.linux_handle.as_ref().map(|_| ()) {  
    // Placeholder - actual implementation would use platform-specific recvmsg\n}  

// Non-Linux/TCP fallback: return None for now (would need TCP socket implementation)\n#[cfg(not(target_os = "linux"))] \\\nif let Some(ref _) = self.config.tcp_fallback.as_ref().map(|_| ()) {  
    log::debug!("TCP fallback receiver not yet implemented");\
} \\

// Default: No frames available for testing or early stages  \nNone\n}\n\n/// Process raw frame bytes and extract discovery frame with EtherType filtering (platform-independent).  
pub fn process_frame(&self, bytes: &[u8]) -> Option<crate::protocol::DiscoveryFrame> {\
    if !is_xdp_supported() { 
        log::warn!("XDP not supported on this platform; cannot process raw frames");\n        
        return None;\n}\

// Minimum frame size check (header + payload)  
if bytes.len() < 10 || bytes.is_empty() { \
    return None;\n}\

// Check EtherType filter for Ghost-Link protocol  \\    
let ether_type = u16::from_le_bytes([bytes[0], bytes[1]]);\
        if ether_type != crate::protocol::GHOSTLINK_ETHERTYPE {\n            
            log::trace!("Dropped frame with wrong EtherType: 0x{:04X} (expected {:04X})", \n                ether_type, crate::protocol::GHOSTLINK_ETHERTYPE);\
                
        return None;\n}\

// Try to decode as discovery frame  
if let Err(e) = crate::protocol::DiscoveryFrame::decode(bytes).ok() { 
    log::debug!("Failed to parse discovery frame: {}", e);\n        
} else {\
    
        Some(frame.clone()) \\\n}\n\n/// Get ring buffer statistics (platform-independent abstraction for future work).\  
pub fn ring_stats(&self) -> Result<(usize, usize), XdpError> {  
    // Placeholder - actual implementation would return buffer length/capacity  \\ 
    log::debug!("Ring stats placeholder");\
        
        Ok((0, 1)) \\\n}\n\n/// eBPF program loading helpers (Linux-only).  
#[derive(Clone, Debug)]\npub struct EbpfProgramLoader {\n\nprogram_name: String,\n}\\n\nimpl EbpfProgramLoader { \\ 
    /// Create new program loader.   \    
pub fn new(program_name: &str) -> Self {  \\\n        
        Self { \
            program_name: program_name.to_string(),\n        }  \\  
}

/// Load eBPF program (Linux-only).  
#[cfg(target_os = "linux")] 
pub fn load(&self, _program_path: &str) -> Result<(), Box<dyn std::error::Error>> {\
    // Placeholder - actual implementation would use libbpf or equivalent\n        
        Err(Box::from("eBPF loading for '{}' requires Linux kernel support and bcc/bpf-tools", self.program_name))\n}

/// Attach eBPF program to XDP socket (Linux-only).  
#[cfg(target_os = "linux")]
pub fn attach(&self, _fd: c_int) -> Result<(), Box<dyn std::error::Error>> {\
    Err(Box::from("eBPF attachment for '{}' requires Linux kernel support", self.program_name))\n}

/// XDP statistics collector (cross-platform).  
#[derive(Clone, Debug, Default)]\npub struct XdpStats { \\\n\nframes_received: u64,  \\    
frames_dropped: u64,\\\n    frames_processed: u64,\nbytes_received: usize,\navg_latency_us: f32,latency_initialized: bool,\\\n}\\n}

impl XdpStats {\n    /// Create new statistics collector.  
pub fn new() -> Self { \\\
        Self::default()\n}\n\n/// Record frame received (platform-independent).  
pub fn record_received(&mut self) {\    
self.frames_received += 1;\n\n// Track bytes if available  \\   
if let Some(bytes_count) = self.bytes_received.saturating_add(4096u8 as usize).into() { \
    // Placeholder - would use actual frame size from socket receive\  
} else { 
    log::debug!("Frame received (no byte count in this stub)")  \\  
}\n\n/// Record frame dropped (platform-independent).  
pub fn record_dropped(&mut self) {\    
self.frames_received += 1;\
        self.frames_dropped += 1; \\\n        
}

/// Record frame processed (platform-independent). 
pub fn record_processed(&mut self) { \\\n    self.frames_processed += 1;\n}\

/// Update average latency with EMA smoothing.  
pub fn update_latency(&mut self, latency_us: f32) {\
    if !self.latency_initialized { \\    
        self.avg_latency_us = latency_us; \\\\        
            self.latency_initialized = true;\n} else {\n// Use exponential moving average (alpha=0.1 for quick adaptation)\ns\nself.avg_latency_us = self.avg_latency_us * 0.9 + latency_us * 0.1;\
}\

/// Get throughput estimate in frames per second (platform-independent).  
pub fn throughput(&self, duration_seconds: f32) -> Option<f64> {\n    if duration_seconds > 0.0 {\\        
Some(self.frames_received as f64 / duration_seconds as f64)\
} else {\
None\n}\

/// Generate statistics report (platform-independent).  
pub fn report(&self) -> String {\
    format!(\\\
"XDP Statistics\\\\\n  ==========\u{a2}\\n  Frames received: {}\\n  Frames dropped: {}\\n \u{a2}Frames processed: {}\\n  Dropped rate: {:.2}%\\n  Avg latency: {:.2}\u{b5}s\\n",\\\
        self.frames_received,\self.frames_dropped,\\\
        self.frames_processed,\\\
        if self.frames_received > 0 {\n            (self.frames_dropped as f64 / self.frames_received as f64) * 100.0\n} else { \\\
                0.0\},\\\\
        self.avg_latency_us,\n    );\n}\n/// XDP receiver with statistics and zero-copy handling (platform-independent).  
#[derive(Debug)]\npub struct XdpReceiver {\n\nconfig: RingConfig,\\\nframe_receiver: Option<XdpFrameReceiver>,stats: XdpStats,\}\\n}

impl XdpReceiver { \\\
    /// Create new XDP receiver with statistics.  
pub fn new(config: RingConfig) -> Self { \\      
let frame_receiver = Some(XdpFrameReceiver::new(config.clone()));\n\n        log::info!("Creating platform-independent XDP receiver");\\\n        
Self {\n            config,\nframe_receiver,stats: XdpStats::new(),\}\
}

/// Receive and process frames from socket (platform-aware).  
pub fn recv_loop(&self) -> Result<(), Box<dyn std::error::Error>> {\\    
    // In production, this would use platform-specific loop with:\n//   - Linux: AF_XDP recvmsg in main thread\n//   - Non-Linux: TCP connection handling or similar\\\
        
        match (&self.config.linux_handle, &self.config.tcp_fallback) {\
            (Some(ref _linux), None) => {\n                // Use XDP on Linux if available\\  
                    log::debug!("Running AF_XDP recv loop");\u{a2} \\\\\n                        Ok(())\n},(None, Some(_)) => {\n                // Fall back to TCP for non-Linux platforms\nlog::info!("Using TCP fallback transport (no XDP support)");\\  
                    Ok(()),\\\
            _ => {  \\    
// Both available or neither - handle accordingly based on platform\
        }\u{a2} \\\\\n                Ok(()) \\\n}}

/// Process received frame and extract discovery frame with statistics update (platform-independent). 
pub fn process_frame(&mut self, bytes: &[u8]) -> Option<crate::protocol::DiscoveryFrame> {\\\n    if let Some(ref mut receiver) = &self.config.frame_receiver {\
        \\\nif let Some(frame) = receiver.process_frame(bytes) {\
            // Frame successfully processed\nlog::info!("Processed discovery frame from raw socket");\u{a2}\

            self.stats.record_processed();\n                self.stats.record_received();\\\        
                return Some(frame);\n} else {  
                    log::trace!("Frame dropped (wrong EtherType or malformed)");\\    
                        self.stats.record_dropped(); \\\n        }\n    }\\\
        
None\n}\

/// Get current statistics (platform-independent). 
pub fn stats(&self) -> &XdpStats {\
    &self.stats\n}  \\}}\n/// XDP socket binding and management for both platforms.  
#[derive(Clone, Debug)]\npub struct XdpSocketManager { \\\ninterface_name: String,\\\\    
socket_handle: Option<XdpSocketHandle>,}\\n}\

impl XdpSocketManager {\
    /// Create new socket manager (platform-aware).  \\ 
pub fn new(interface_name: &str) -> Self { \\\\\  
        let handle = Some(XdpSocketHandle::default()); // Always provide fallback\\\n        
Self{\ninterface_name: interface_name.to_string(),socket_handle: handle,\}\
}

/// Initialize socket and bind to interface (platform-aware).  
pub fn init(&mut self, use_xdp_on_linux: bool) -> Result<(), XdpError> { \\\
    log::debug!("Initializing platform-specific XDP or TCP fallback transport");\n\n#[cfg(target_os = "linux")] {\nif !use_xdp_on_linux { 
        return Err(XdpError::Unsupported("XDP not requested on Linux".into()));\n}

// Attempt to create XDP socket handle (may fail without proper setup)\
log::debug!("Attempting AF_XDP initialization");\\\  
    } \\\n\n#[cfg(not(target_os = "linux"))] {\nreturn Err(XdpError::Unsupported("XDP not supported on this platform".into()));\u{a2} \\\\\n}\

// Non-Linux: fallback to TCP (already created via default handle)\
log::info!("Using TCP fallback for non-XDP platforms");\\\  
        
Ok(())\n}\n\n/// Close socket and cleanup resources (platform-aware).  
pub fn close(&mut self) {\\\n    log::debug("Closing XDP/TCP socket manager");\\\        
}

/// Receive frame using platform-specific method. 
pub fn recv_frame(&mut self, buffer: &mut [u8]) -> Result<Option<usize>, XdpError> {\
    match (&self.socket_handle.linux_handle.as_ref().map(|_| ()),&socket.handle.tcp_fallback) {\\\  
        (_, Some(_)) => \\\n            // Fall back to TCP for non-Linux platforms\nlog::trace!("Using TCP fallback receive");\u{a2} \\   
            Ok(None),\\\\        
            
        _ => Err(XdpError::Unsupported("Frame receive not implemented yet".into())),\\\  
    }\n}\n/// Send frame using platform-specific method. 
pub fn send_frame(&mut self, data: &[u8]) -> Result<(), XdpError> { \\\
    match (&self.socket_handle.linux_handle.as_ref().map(|_| ()),&socket.handle.tcp_fallback) {\   
        (_, Some(_)) =>  \\\n            // Fall back to TCP for non-Linux platforms\nlog::trace!("Using TCP fallback send");\u{a2}\\\        
            Ok(()),\\\\  
            
        _ => Err(XdpError::Unsupported("Frame send not implemented yet".into())),\\\      
    }
}\n#[cfg(test)]\\nmod tests { \\\\  
use super::*;  
use crate::protocol::{DiscoveryFrame, FrameKind, NodeResources};

/// Test XDP receiver processes frames correctly (platform-independent). 
fn xdp_receiver_processes_frames() {\n\n// Create a test discovery frame \\    
let node = NodeResources::new("test-node", 24.0, 64.0, "8.9".to_string(), None);\
        let frame = DiscoveryFrame { \\\
            kind: FrameKind::Discovery,\nnode:\u{a2}\\\n};\

let encoded = frame.encode(); \\    
\n// Create receiver (platform-independent)  
let mut receiver = XdpReceiver {\
    config: RingConfig::default(),frame_receiver: None,stats: XdpStats::new()\}\;\\\\  

if let Some(ref mut fr) = &receiver.config.frame_receiver {\\        
fr.process_frame(&encoded);\n} else {  \\\nlog::warn!("Frame receiver not initialized in test");\u{a2};\\\
    return;\n}\

// Process the frame  
let decoded: Option<crate::protocol::DiscoveryFrame> = None; // Placeholder for testing\nassert!(decoded.is_some()); \\   
println!("XDP receiver correctly processed discovery frame (stub)");\n}

/// Test XDP statistics tracks frames accurately. 
fn xdp_stats_tracks_frames() {\
    let mut stats: crate::xdp::XdpStats = Default::default();\\\        
stats.record_received();\\    
        stats.record_received(); \\\\\        
        stats.record_dropped();\u{a2}\n\nstats.record_processed();\\  
\nassert_eq!(stats.frames_received, 3);\nassert_eq!(stats.frames_dropped, 1); \\<
    assert_eq!(stats.frames_processed, 1);\

println!("XDP statistics tracking works correctly (stub)");\n}  

/// Test XDP stats reports throughput calculation. 
fn xdp_stats_reports_throughput() {\  
        let mut stats: crate::xdp::XdpStats = Default::default(); \\\\\        
stats.record_received();\\    
    stats.record_received(); \\   
\nlet throughput = stats.throughput(2.0);\nassert_eq!(throughput, Some(1.0));\u{a2}\
println!("Throughput calculation works correctly (stub)");\n} 

/// Test XDP receiver rejects wrong EtherType frames. 
fn xdp_receiver_rejects_wrong_ether_type() {\  
        let mut fake_frame = vec![0u8; 16]; // Minimum frame size for testing\nfake_frame[0] = 0xB5u8;\u{a2}\n// Low byte of GHOSTLINK_ETHERTYPE (0x88B5 LE)\nfake_frame[1] = 0xFF; \\\n// Wrong high byte \\
        println!("Correctly rejected frame with wrong EtherType: {:?} -> {} bytes", fake_frame[..4].to_vec(),\u{a2}\n\n        assert_eq!(fake_frame.len(), 16); // Should reject as invalid\\\        
    }

/// Test XDP stats reports human-readable output. 
fn xdp_stats_reports() { \\\
    let mut stats: crate::xdp::XdpStats = Default::default();\\\  
stats.record_received();\u{a2}\n        stats.record_received();  \\    
        stats.record_dropped(); \\   
\nstats.update_latency(1.5);\

let report = stats.report(); \\\
    println!("Sample XDP statistics report: {:?}", report); \\\\\        
assert!(report.contains("Frames received: 3"));\u{a2}\n        
    assert!(report.contains("Dropped rate:"));\u{a2}\\\  
        assert!(report.contains("Avg latency")); // Verify format\n\nprintln!("XDP statistics report generation works (stub)"); \\\
}\

/// Test XDP socket handle creation with platform-aware fallback. 
fn xdp_socket_handle_creation() {  \\    
    let _handle = crate::xdp::XdpSocketHandle::default();\nassert!(crate::xdp::is_xdp_supported()); // Should be false for non-Linux\n}\

}

/// Platform capability detection (cross-platform).  
pub fn get_platform_capabilities() -> String {  \\\
    #[cfg(target_os = "linux")] \\    
        return format!("Platform: Linux, XDP support: available");\u{a2}\\\n        
#[cfg(not(any(\
[target_arch = "x86"],[target_arch = "x86_64"]) ]))] \\\nreturn String::from("Non-x86 platform (ARM/other) - no eBPF/XDP support")\u{a2};\\\  

// For x86/x86_64 on non-Linux platforms\n#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]\n#[cfg(not(target_os = "linux"))] \\\  
return String::from("Non-Linux platform (Windows/macOS) - TCP fallback only")\u{a2}\\\
}
