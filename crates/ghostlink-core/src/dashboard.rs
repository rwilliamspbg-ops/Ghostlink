#[derive(Clone, Debug, PartialEq)]
pub struct NodeMetrics {
    pub name: String,
    pub gpu_name: String,
    pub used_vram_gb: f32,
    pub total_vram_gb: f32,
    pub streaming_layers: Option<(usize, usize)>,
    pub af_xdp_gbps: f32,
    pub latency_micros: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DashboardSnapshot {
    pub status: String,
    pub ring_fill_percent: u8,
    pub gradient_steps: u64,
    pub nodes: Vec<NodeMetrics>,
}

impl DashboardSnapshot {
    pub fn render_ascii(&self) -> String {
        let mut output =
            String::from("+───────────────────────────────────────────────────────────────+\n");
        output.push_str(&format!(
            "| GHOST-LINK CLUSTER DASHBOARD               [STATUS: {:<8}] |\n",
            self.status
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
                node.name, node.gpu_name, gauge, node.used_vram_gb, node.total_vram_gb
            ));

            if let Some((start, end)) = node.streaming_layers {
                output.push_str(&format!(
                    "| >>> Streaming Layers {:>2}-{:>2} >>> [AF_XDP: {:>4.1} Gbps / {:>3.1}μs] |\n",
                    start, end, node.af_xdp_gbps, node.latency_micros
                ));
            }
        }

        output.push_str("+───────────────────────────────────────────────────────────────+\n");
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_dashboard_with_stream_information() {
        let snapshot = DashboardSnapshot {
            status: "ACTIVE".into(),
            ring_fill_percent: 63,
            gradient_steps: 42,
            nodes: vec![NodeMetrics {
                name: "NODE-01".into(),
                gpu_name: "RTX4090".into(),
                used_vram_gb: 22.4,
                total_vram_gb: 24.0,
                streaming_layers: Some((0, 24)),
                af_xdp_gbps: 9.8,
                latency_micros: 1.2,
            }],
        };

        let rendered = snapshot.render_ascii();

        assert!(rendered.contains("GHOST-LINK CLUSTER DASHBOARD"));
        assert!(rendered.contains("Streaming Layers  0-24"));
        assert!(rendered.contains("Ring Buffer Fill:  63%"));
    }
}
