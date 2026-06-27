//! Mohawk GUI Dashboard
//!
//! Terminal-based UI for monitoring distributed inference clusters

use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Widget},
    Terminal,
};
use std::{io, time::Duration};

/// Main dashboard application
pub struct Dashboard {
    node_id: String,
    cluster_nodes: usize,
    healthy_nodes: usize,
    throughput: f32,
    latency_ms: f32,
}

impl Dashboard {
    pub fn new() -> Self {
        Self {
            node_id: "node-0000".to_string(),
            cluster_nodes: 0,
            healthy_nodes: 0,
            throughput: 0.0,
            latency_ms: 0.0,
        }
    }

    /// Update dashboard metrics
    pub fn update_metrics(
        &mut self,
        cluster_nodes: usize,
        healthy_nodes: usize,
        throughput: f32,
        latency_ms: f32,
    ) {
        self.cluster_nodes = cluster_nodes;
        self.healthy_nodes = healthy_nodes;
        self.throughput = throughput;
        self.latency_ms = latency_ms;
    }

    /// Run the dashboard UI
    pub fn run(&mut self) -> Result<()> {
        // Setup terminal
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        // Main loop
        loop {
            terminal.draw(|f| {
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .margin(1)
                    .constraints([
                        Constraint::Length(3),
                        Constraint::Length(10),
                        Constraint::Length(10),
                        Constraint::Min(0),
                    ])
                    .split(f.size());

                // Header
                let header = Paragraph::new(Line::from(vec![
                    Span::styled(
                        "🦅 Mohawk Inference Engine",
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(" | "),
                    Span::raw(format!("Node: {}", self.node_id)),
                ]))
                .block(Block::default().borders(Borders::ALL).title("Header"));
                f.render_widget(header, chunks[0]);

                // Cluster Status
                let cluster_info = vec![
                    Line::from(format!("Cluster Nodes: {}", self.cluster_nodes)),
                    Line::from(format!("Healthy Nodes: {}", self.healthy_nodes)),
                    Line::from(format!("Throughput: {:.1} req/s", self.throughput)),
                    Line::from(format!("Latency: {:.2} ms", self.latency_ms)),
                ];
                let cluster = Paragraph::new(cluster_info).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("Cluster Status"),
                );
                f.render_widget(cluster, chunks[1]);

                // Network Health
                let health_info =
                    if self.healthy_nodes == self.cluster_nodes && self.cluster_nodes > 0 {
                        vec![Line::from(Span::styled(
                            "✓ All nodes healthy",
                            Style::default().fg(Color::Green),
                        ))]
                    } else if self.cluster_nodes == 0 {
                        vec![Line::from(Span::styled(
                            "○ Waiting for discovery...",
                            Style::default().fg(Color::Yellow),
                        ))]
                    } else {
                        vec![Line::from(Span::styled(
                            "⚠ Some nodes unhealthy",
                            Style::default().fg(Color::Red),
                        ))]
                    };
                let health = Paragraph::new(health_info).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("Network Health"),
                );
                f.render_widget(health, chunks[2]);

                // Help
                let help = Paragraph::new("Press 'q' to quit | Press 'r' to refresh")
                    .block(Block::default().borders(Borders::ALL).title("Help"));
                f.render_widget(help, chunks[3]);
            })?;

            // Handle input
            if event::poll(Duration::from_millis(250))? {
                if let Event::Key(key) = event::read()? {
                    match key.code {
                        event::KeyCode::Char('q') => break,
                        event::KeyCode::Char('r') => {
                            // Refresh metrics (in real impl, would fetch from API)
                        }
                        _ => {}
                    }
                }
            }
        }

        // Restore terminal
        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;
        terminal.show_cursor()?;

        Ok(())
    }
}

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let mut dashboard = Dashboard::new();

    println!("Starting Mohawk GUI Dashboard...");
    println!("Press 'q' to quit");

    dashboard.run()?;

    Ok(())
}
