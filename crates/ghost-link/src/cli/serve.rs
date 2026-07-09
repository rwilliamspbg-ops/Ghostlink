//! Serve command implementation (API server)

use anyhow::Result;

/// Start OpenAI-compatible API server
pub fn run(host: String, port: u16) -> Result<()> {
    println!("Starting API server on {}:{}...", host, port);
    
    // TODO: Implement actual API server startup
    // Uses axum to create HTTP server with endpoint handlers
    
    println!("Server started. Press Ctrl+C to stop.");
    
    // Keep running...
    std::thread::sleep(std::time::Duration::MAX);
    
    Ok(())
}
