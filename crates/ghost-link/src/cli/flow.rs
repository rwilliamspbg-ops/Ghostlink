//! Flow command implementation

use anyhow::Result;

/// Run full 30B planning flow
pub fn execute(
    local_id: String,
    remote_id: String,
    remote_vram_gb: Option<f32>,
    remote_mem_gb: Option<f32>,
    exec_tokens: Option<usize>,
    micro_batch: Option<usize>,
    transport: String,
) -> Result<()> {
    println!("Running 30B planning flow...");
    println!("Local: {}", local_id);
    println!("Remote: {}", remote_id);
    
    // TODO: Implement actual flow execution
    
    Ok(())
}
