//! Mohawk Engine CLI
//! 
//! Main entry point for the Mohawk Inference Engine

use anyhow::Result;
use mohawk_engine::{InferenceEngine, EngineConfig};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "mohawk-engine")]
#[command(about = "Mohawk Inference Engine - Distributed ML Inference")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
    
    /// Network interface to use (auto-detect if not specified)
    #[arg(long, default_value = "auto")]
    interface: String,
    
    /// Node ID (auto-generated if not specified)
    #[arg(long)]
    node_id: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the inference engine
    Start {
        /// Model path to load
        #[arg(short, long)]
        model: Option<String>,
        
        /// API server address
        #[arg(short, long, default_value = "0.0.0.0:8003")]
        addr: String,
    },
    
    /// Join an existing cluster
    Join {
        /// Cluster address to join
        #[arg(short, long)]
        cluster: String,
    },
    
    /// Run inference on a model
    Infer {
        /// Model ID
        #[arg(short, long)]
        model: String,
        
        /// Input data file
        #[arg(short, long)]
        input: String,
    },
    
    /// Show cluster status
    Status,
    
    /// List loaded models
    Models,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    
    // Initialize logging
    tracing_subscriber::fmt::init();
    
    match cli.command {
        Commands::Start { model, addr } => {
            let mut config = EngineConfig::default();
            if let Some(node_id) = cli.node_id {
                config.node_id = node_id;
            }
            if cli.interface != "auto" {
                config.interface = Some(cli.interface);
            }
            if let Some(model_path) = model {
                config.model_path = model_path;
            }
            
            let mut engine = InferenceEngine::new(config)?;
            
            // Start auto-discovery
            engine.start_discovery().await?;
            
            // Load model if specified
            if !engine.config.model_path.is_empty() {
                engine.load_model(&engine.config.model_path)?;
            }
            
            // Start API server
            mohawk_engine::api::start_api_server(&addr).await?;
            
            println!("Mohawk Engine started on {}", addr);
            println!("Node ID: {}", engine.config.node_id);
            println!("Cluster nodes: {}", engine.cluster().node_count());
            
            // Keep running
            tokio::signal::ctrl_c().await?;
        }
        
        Commands::Join { cluster } => {
            println!("Joining cluster at: {}", cluster);
            // TODO: Implement cluster join logic
        }
        
        Commands::Infer { model, input } => {
            println!("Running inference on model '{}' with input '{}'", model, input);
            // TODO: Implement inference logic
        }
        
        Commands::Status => {
            println!("Cluster Status");
            println!("==============");
            // TODO: Show cluster status
        }
        
        Commands::Models => {
            println!("Loaded Models");
            println!("=============");
            // TODO: List models
        }
    }
    
    Ok(())
}
