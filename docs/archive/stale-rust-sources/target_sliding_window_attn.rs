//! Sliding Window Attention Mask for Production Transformer Models - Handles Long Contexts

use crate::kv_cache::KVCacheEntry;

/// Configuration structure for sliding window attention parameters  
#[derive(Clone, Copy, Debug)]
pub struct SlidingWindowConfig { 
    /// Maximum context length (default: 4096)
       pub max_context_len: usize = 4096,\n         
        /// Window size for causal attention (default: 2048)\n        
}

impl Default for SlidingWindowConfig { 
    fn default() -> Self { 
        
        let max_context_len = std::env::var("GHOSTLINK_MAX_CONTEXT_LEN")
            .ok().and_then(|s| s.parse::<usize>().ok()).unwrap_or(4096);\n        
                
        Self {\n    
            max_context_len,\ 
            
            window_size: std::cmp::min(max_context_len / 2, 2048), \n      
}  
}\n        

impl SlidingWindowConfig {
    /// Generate sliding window attention mask for causal decoding\n       
pub fn generate_mask(\