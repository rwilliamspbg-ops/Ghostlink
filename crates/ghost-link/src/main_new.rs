//! Ghost-Link CLI Demo
//!
//! Command-line interface for Ghost-Link primitives.
//!
//! Note: This is the NEW refactored version. The old main.rs has been
//! extracted into modules in the cli/ and api/ directories.

use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::cli::{
    plan, join, listen, dashboard, doctor, flow, serve, gui,
};

/// Ghost-Link CLI - High-performance network fabric for distributed inference
#[derive(Parser)]
#[command(name = "ghost-link")]
#[command(about = "Ghost-Link: Zero-config LAN fabric for shared GPU inference", long_about = None)]
struct Cli {
    /// Command to execute
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate layer placement plan
    Plan,
    
    /// Broadcast discovery frame to join cluster
    Join {
        /// Target node ID
        node_id: String,
    },
    
    /// Reply to UDP discovery requests
    Listen {
        /// Node ID
        node_id: String,
        /// Reply once only
        #[arg(short, long)]
        once: bool,
    },
    
    /// Display ASCII cluster dashboard
    Dashboard,
    
    /// Run unified troubleshooting checks
    Doctor {
        /// Strict mode
        #[arg(short, long)]
        strict: bool,
        /// Output as JSON
        #[arg(short, long)]
        json: Option<String>,
        /// Network probe target
        #[arg(long)]
        network_probe: bool,
        /// Network target address
        #[arg(long)]
        network_target: Option<String>,
    },
    
    /// Run full 30B planning flow
    Flow {
        /// Local node ID
        local_id: String,
        /// Remote node ID
        remote_id: String,
        /// Remote VRAM in GB
        #[arg(short, long)]
        remote_vram_gb: Option<f32>,
        /// Remote system memory in GB
        #[arg(short, long)]
        remote_mem_gb: Option<f32>,
        /// Execution tokens
        #[arg(short, long)]
        exec_tokens: Option<usize>,
        /// Micro-batch size
        #[arg(short, long)]
        micro_batch: Option<usize>,
        /// Transport mode
        #[arg(short, long, default_value = "tcp")]
        transport: String,
    },
    
    /// Start OpenAI-compatible API server
    Serve {
        /// Host address
        host: String,
        /// Port number
        port: u16,
    },
    
    /// Launch vendored Mohawk GUI (Python/PyQt6)
    Gui {
        #[arg(num_args = 0..)]
        args: Vec<String>,
    },
    
    /// Check GUI readiness
    GuiCheck {
        /// Strict validation
        #[arg(short, long)]
        strict: bool,
    },
    
    /// Diagnose GUI issues
    GuiDiagnose {
        /// Strict mode
        #[arg(short, long)]
        strict: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    
    match cli.command {
        Commands::Plan => plan::execute(),
        Commands::Join { node_id } => join::execute(node_id),
        Commands::Listen { node_id, once } => listen::execute(node_id, once),
        Commands::Dashboard => dashboard::execute(),
        Commands::Doctor { strict, json, network_probe, network_target } => {
            doctor::execute(strict, json.as_deref(), network_probe, network_target.as_deref())
        },
        Commands::Flow { local_id, remote_id, remote_vram_gb, remote_mem_gb, 
                       exec_tokens, micro_batch, transport } => {
            flow::execute(local_id, remote_id, remote_vram_gb, remote_mem_gb,
                         exec_tokens, micro_batch, transport)
        },
        Commands::Serve { host, port } => serve::run(host, port),
        Commands::Gui { args } => gui::launch(args),
        Commands::GuiCheck { strict } => gui::check(strict),
        Commands::GuiDiagnose { strict } => gui::diagnose(strict),
    }?;
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_cli_parsing() {
        let cli = Cli::parse_from(&["ghost-link", "plan"]);
        assert!(matches!(cli.command, Commands::Plan));
    }
}
