//! Ghost-Link CLI Subcommand Dispatch
//!
//! This module dispatches to individual subcommand implementations.

use anyhow::Result;

use crate::cli::{
    plan, join, listen, dashboard, doctor, flow, serve, gui,
};

/// Main CLI entry point
pub fn execute_command(command: CliCommand) -> Result<()> {
    match command {
        CliCommand::Plan => plan::execute(),
        CliCommand::Join { node_id } => join::execute(node_id),
        CliCommand::Listen { node_id, once } => listen::execute(node_id, once),
        CliCommand::Dashboard => dashboard::execute(),
        CliCommand::Doctor { strict, json, network_probe, network_target } => {
            doctor::execute(strict, json.as_deref(), network_probe, network_target.as_deref())
        },
        CliCommand::Flow { local_id, remote_id, remote_vram_gb, remote_mem_gb, 
                       exec_tokens, micro_batch, transport } => {
            flow::execute(local_id, remote_id, remote_vram_gb, remote_mem_gb,
                         exec_tokens, micro_batch, transport)
        },
        CliCommand::Serve { host, port } => serve::run(host, port),
        CliCommand::Gui { args } => gui::launch(args),
        CliCommand::GuiCheck { strict } => gui::check(strict),
        CliCommand::GuiDiagnose { strict } => gui::diagnose(strict),
    }?;
    
    Ok(())
}

#[derive(Debug, PartialEq)]
pub enum CliCommand {
    Plan,
    Join { node_id: String },
    Listen { node_id: String, once: bool },
    Dashboard,
    Doctor { 
        strict: bool,
        json: Option<String>,
        network_probe: bool,
        network_target: Option<String>,
    },
    Flow { 
        local_id: String,
        remote_id: String,
        remote_vram_gb: Option<f32>,
        remote_mem_gb: Option<f32>,
        exec_tokens: Option<usize>,
        micro_batch: Option<usize>,
        transport: String,
    },
    Serve { host: String, port: u16 },
    Gui { args: Vec<String> },
    GuiCheck { strict: bool },
    GuiDiagnose { strict: bool },
}
