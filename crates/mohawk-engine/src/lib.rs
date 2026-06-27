//! Mohawk Inference Engine - High-performance distributed inference
//! 
//! This crate integrates Ghost-Link networking with the Mohawk inference engine,
//! providing:
//! - Auto-discovery of inference nodes via AF_PACKET sockets
//! - Distributed model layer splitting across workers
//! - Real-time metrics and health monitoring
//! - Secure JWT/mTLS authentication

pub mod engine;
pub mod worker;
pub mod session;
pub mod api;
pub mod metrics;

pub use engine::InferenceEngine;
pub use worker::WorkerConfig;
pub use session::InferenceSession;
pub use metrics::EngineMetrics;

/// Engine version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Initialize the Mohawk Engine with Ghost-Link networking
pub fn init() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    tracing::info!("Mohawk Inference Engine v{} initialized", VERSION);
    Ok(())
}
