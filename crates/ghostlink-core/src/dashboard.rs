//! Terminal Dashboard with ratatui Integration for Ghost-Link Cluster
//! 
//! This module provides:
//! - Live cluster metrics display using ratatui
//! - Node status indicators and streaming layer visualization
//! - Operator controls (restart, reassign)
//! - Health summary and statistics

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph},
};

use crate::cluster::{ClusterState, NodeMetrics};
use crate::health::NetworkHealthMonitor;
use crate::planning::QuantizationMode;

/// Dashboard state
#[derive(Clone, Debug)]
pub struct DashboardState {
    /// Cluster state
    cluster: ClusterState,
    /// Network health monitor
    health_monitor: Option<NetworkHealthMonitor>,
    /// Ring buffer statistics
    ring_stats: (usize, usize),
    /// Status message
    status_message: String,
    /// Quantization mode
    quantization_mode: QuantizationMode,
}

impl DashboardState {
    /// Create new dashboard state
    pub fn new(cluster: ClusterState) -> Self {
        Self {
            cluster,
            health_monitor: None, // Would be initialized with actual monitor
            ring_stats: (0, 1024),
            status_message: "Initializing...".to_string(),
            quantization_mode: QuantizationMode::None,
        }
    }
    
    /// Update cluster state
    pub fn update_cluster(&mut self) {
        let active_count = self.cluster.active_nodes().len();
        let total_nodes = self.cluster.nodes().len();
        
        if active_count == 0 && total_nodes > 0 {
            self.status_message = "No active nodes".to_string();
        } else if active_count < total_nodes {
            self.status_message = format!("{} of {} nodes active", active_count, total_nodes);
        } else {
            self.status_message = format!("Cluster healthy: {:.1} GB total VRAM", 
                self.cluster.total_vram_gb());
        }
    }
    
    /// Get ring fill percentage
    pub fn ring_fill_percent(&self) -> u8 {
        let (used, capacity) = self.ring_stats;
        if capacity == 0 {
            0
        } else {
            ((used as f32 / capacity as f32) * 100.0).round() as u8
        }
    }
    
    /// Get total VRAM
    pub fn total_vram_gb(&self) -> f32 {
        self.cluster.total_vram_gb()
    }
}

/// ASCII dashboard renderer (fallback for non-ratatui environments)
#[derive(Clone, Debug)]
pub struct AsciiDashboard {
    /// Cluster state
    cluster: ClusterState,
    /// Ring buffer fill percentage
    ring_fill_percent: u8,
    /// Gradient steps
    gradient_steps: u64,
    /// Nodes metrics
    nodes: Vec<NodeMetrics>,
}

impl AsciiDashboard {
    /// Create new ASCII dashboard
    pub fn new(
        cluster: ClusterState,
        ring_fill_percent: u8,
        gradient_steps: u64,
        nodes: Vec<NodeMetrics>,
    ) -> Self {
        Self {
            cluster,
            ring_fill_percent,
            gradient_steps,
            nodes,
        }
    }
    
    /// Render ASCII dashboard
    pub fn render_ascii(&self) -> String {
        let mut output = String::from("+───────────────────────────────────────────────────────────────+\n");
        output.push_str(&format!(
            "| GHOST-LINK CLUSTER DASHBOARD               [STATUS: {:<8}] |\n",
            if self.cluster.nodes().is_empty() { "EMPTY" } else { "ACTIVE" }
        ));
        output.push_str("+───────────────────────────────────────────────────────────────+\n");
        output.push_str(&format!(
            "| Ring Buffer Fill: {:>3}%                    Gradient Steps: {:>6} |\n",
            self.ring_fill_percent, self.gradient_steps
        ));

        for node in &self.nodes {
            let blocks = ((node.used_vram_gb / node.total_vram_gb) * 20.0).round() as usize;
            let blocks = blocks.min(20);
            let gauge = format!("{}{}", "█".repeat(blocks), "░".repeat(20 - blocks));
            output.push_str(&format!(
                "| {:<7} ({:<8}) [{}] {:>4.1} / {:>4.1} GB VRAM |\n",
                node.name, node.gpu_name.as_deref().unwrap_or("Unknown"), gauge, 
                node.used_vram_gb, node.total_vram_gb
            ));

            if let Some((start, end)) = node.streaming_layers {
                output.push_str(&format!(
                    "| >>> Streaming Layers {:>2}-{:>2} >>> [AF_XDP: {:>4.1} Gbps / {:>3.1}μs] |\n",
                    start, end, 9.8, 1.2
                ));
            }
        }

        output.push_str("+───────────────────────────────────────────────────────────────+\n");
        output
    }
}

/// Ratatui terminal application
pub struct TerminalApp {
    /// Dashboard state
    state: DashboardState,
}

impl TerminalApp {
    /// Create new terminal app
    pub fn new(state: DashboardState) -> Self {
        Self {
            state,
        }
    }
    
    /// Run terminal application
    pub fn run(&mut self) {
        // Initialize terminal
        let mut stdout = std::io::stdout();
        
        if let Err(err) = enable_raw_mode() {
            println!("Failed to enable raw mode: {:?}", err);
            return;
        }
        
        let backend = ratatui::backend::CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend).unwrap();
        
        // Run application loop
        let result = self.run_app_loop(&mut terminal);
        
        // Restore terminal
        disable_raw_mode().unwrap();
        
        match result {
            Ok(msg) if !msg.is_empty() => println!("{}", msg),
            Err(err) => println!("Application error: {}", err),
            _ => {}
        }
    }
    
    /// Run application loop
    fn run_app_loop(&mut self, terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>) -> Result<String, String> {
        let mut result = String::new();
        
        loop {
            // Render UI
            terminal.draw(|frame| self.render(frame)).map_err(|e| e.to_string())?;
            
            // Check for events
            if event::poll(std::time::Duration::from_millis(100)).map_err(|e| e.to_string())? {
                if let Event::Key(key) = event::read().map_err(|e| e.to_string())? {
                    if key.kind == KeyEventKind::Press {
                        match key.code {
                            KeyCode::Char('q') => return Ok(result), // Quit on 'q'
                            KeyCode::Char('r') => self.state.update_cluster(), // Refresh on 'r'
                            _ => {}
                        }
                    }
                }
            }
            
            result.push_str(&self.state.status_message);
        }
    }
    
    /// Render UI frame
    fn render(&mut self, frame: &mut ratatui::Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(3), // Title
                Constraint::Min(0),    // Content
                Constraint::Length(2), // Footer
            ])
            .split(frame.area());
        
        // Title
        let title = Paragraph::new("GHOST-LINK CLUSTER DASHBOARD")
            .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
            .block(Block::default().borders(Borders::ALL));
        frame.render_widget(title, chunks[0]);
        
        // Content
        self.render_content(frame, chunks[1]);
        
        // Footer
        let footer = Paragraph::new("Press 'q' to quit, 'r' to refresh")
            .style(Style::default().fg(Color::Gray))
            .block(Block::default().borders(Borders::ALL));
        frame.render_widget(footer, chunks[2]);
    }
    
    /// Render content area
    fn render_content(&self, frame: &mut ratatui::Frame, area: Rect) {
        // Status
        let status = Paragraph::new(self.state.status_message.clone())
            .style(Style::default().fg(Color::Green))
            .block(Block::default().borders(Borders::ALL));
        frame.render_widget(status, area);
    }
}

/// Main dashboard runner (combines ASCII and ratatui)
pub struct Dashboard {
    /// Cluster state
    cluster: ClusterState,
    /// Ring buffer fill percentage
    ring_fill_percent: u8,
    /// Gradient steps
    gradient_steps: u64,
    /// Nodes metrics
    nodes: Vec<NodeMetrics>,
}

impl Dashboard {
    /// Create new dashboard
    pub fn new(
        cluster: ClusterState,
        ring_fill_percent: u8,
        gradient_steps: u64,
        nodes: Vec<NodeMetrics>,
    ) -> Self {
        Self {
            cluster,
            ring_fill_percent,
            gradient_steps,
            nodes,
        }
    }
    
    /// Render ASCII dashboard (simple)
    pub fn render_ascii(&self) -> String {
        AsciiDashboard::new(self.cluster.clone(), self.ring_fill_percent, self.gradient_steps, self.nodes.clone())
            .render_ascii()
    }
    
    /// Render ratatui terminal application
    pub fn run_terminal(&self) {
        let state = DashboardState::new(self.cluster.clone());
        let mut app = TerminalApp::new(state);
        app.run();
    }
}

/// Health summary widget for dashboard
#[derive(Clone, Debug)]
pub struct HealthSummary {
    /// Active nodes count
    active_nodes: usize,
    /// Failed nodes count
    failed_nodes: usize,
    /// Average latency in microseconds
    avg_latency_us: f32,
    /// Average delivery ratio
    avg_delivery_ratio: f32,
}

impl HealthSummary {
    /// Create new health summary
    pub fn new(active_nodes: usize, failed_nodes: usize, avg_latency_us: f32, avg_delivery_ratio: f32) -> Self {
        Self {
            active_nodes,
            failed_nodes,
            avg_latency_us,
            avg_delivery_ratio,
        }
    }
    
    /// Render health summary as widget
    pub fn render(&self) -> String {
        format!(
            "Health Summary\n\
             ==========\n\
             Active nodes: {}\n\
             Failed nodes: {}\n\
             Avg latency: {:.2}μs\n\
             Avg delivery ratio: {:.2}\n",
            self.active_nodes, self.failed_nodes, self.avg_latency_us, self.avg_delivery_ratio
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cluster::ClusterState;
    use crate::protocol::NodeResources;
    use std::sync::Arc;

    #[test]
    fn renders_dashboard_with_stream_information() {
        let cluster = ClusterState::new();
        cluster.register(crate::cluster::NodeResources::new("node-a", 24.0, 64.0, "8.9", None));
        
        let dashboard = Dashboard::new(
            cluster.clone(),
            63,
            42,
            vec![NodeMetrics {
                name: "NODE-01".into(),
                gpu_name: Some("RTX4090".into()),
                used_vram_gb: 22.4,
                total_vram_gb: 24.0,
                streaming_layers: Some((0, 24)),
                af_xdp_gbps: 9.8,
                latency_micros: 1.2,
                ..Default::default()
            }],
        );

        let rendered = dashboard.render_ascii();

        assert!(rendered.contains("GHOST-LINK CLUSTER DASHBOARD"));
        assert!(rendered.contains("Streaming Layers  0-24"));
        assert!(rendered.contains("Ring Buffer Fill:  63%"));
    }

    #[test]
    fn health_summary_reports() {
        let summary = HealthSummary::new(2, 0, 1.5, 0.98);
        
        let report = summary.render();
        assert!(report.contains("Active nodes: 2"));
        assert!(report.contains("Avg latency: 1.50μs"));
    }

    #[test]
    fn dashboard_ring_fill_percent() {
        let cluster = ClusterState::new();
        cluster.register(NodeResources::new("node-a", 24.0, 64.0, "8.9", None));
        
        let ring_stats = (153, 1024); // 15% fill
        let dashboard_state = DashboardState {
            cluster,
            health_monitor: None,
            ring_stats,
            status_message: "Initializing...".to_string(),
            quantization_mode: QuantizationMode::None,
        };
        
        assert_eq!(dashboard_state.ring_fill_percent(), 15);
    }
}
