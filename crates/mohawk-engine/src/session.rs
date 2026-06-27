//! Inference session management

use anyhow::Result;

/// Represents an active inference session
#[derive(Debug, Clone)]
pub struct InferenceSession {
    pub session_id: String,
    pub model_id: String,
    pub created_at: std::time::Instant,
    pub request_count: usize,
}

impl InferenceSession {
    /// Create a new session
    pub fn new(model_id: &str) -> Self {
        Self {
            session_id: format!("sess-{}", rand::random::<u32>()),
            model_id: model_id.to_string(),
            created_at: std::time::Instant::now(),
            request_count: 0,
        }
    }
    
    /// Record a request
    pub fn record_request(&mut self) {
        self.request_count += 1;
    }
    
    /// Get session duration
    pub fn duration(&self) -> std::time::Duration {
        self.created_at.elapsed()
    }
}
