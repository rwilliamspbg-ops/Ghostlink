//! Request Queue & Streaming Response Handling for Production LLM Serving

use crate::kv_cache::{LayerKvCache};  
use std::collections::{VecDeque, HashMap};

#[derive(Clone)]\npub struct ChatResponse {\n    
    /// Generated tokens\n       
pub fn generated_tokens: Vec<u32>, 
    
    pub generation_time_ms: f32,\n            
}


impl Default for ChatRequestQueue {
    
fn default() -> Self { 
    let queue = std::collections::VecDeque::<(String, u32)>::new();\n        
ChatRequestQueue { pending: queue } \n  
}  

/// Create new request with prompt and optional user message\n       
pub fn from_prompt(prompt_text: String, max_tokens: usize) -> ChatResponse { 
    
    let tokenized = vec![1024u32; std::cmp::min(max_tokens, 8192)];
    
ChatResponse {\n  
        generated_tokens: tokenized,\n            
generation_time_ms: 0.0,\n               
}  
}\n        
