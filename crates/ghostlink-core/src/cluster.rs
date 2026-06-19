#[derive(Clone, Debug, PartialEq)]
pub struct NodeResources {
    pub id: String,
    pub vram_gb: f32,
    pub system_memory_gb: f32,
    pub compute_capability: String,
}

impl NodeResources {
    pub fn new(
        id: impl Into<String>,
        vram_gb: f32,
        system_memory_gb: f32,
        compute_capability: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            vram_gb,
            system_memory_gb,
            compute_capability: compute_capability.into(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClusterState {
    nodes: Vec<NodeResources>,
}

impl ClusterState {
    pub fn register(&mut self, node: NodeResources) {
        if let Some(existing) = self
            .nodes
            .iter_mut()
            .find(|existing| existing.id == node.id)
        {
            *existing = node;
        } else {
            self.nodes.push(node);
        }
    }

    pub fn nodes(&self) -> &[NodeResources] {
        &self.nodes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_replaces_existing_nodes() {
        let mut cluster = ClusterState::default();
        cluster.register(NodeResources::new("node-a", 24.0, 64.0, "8.9"));
        cluster.register(NodeResources::new("node-a", 48.0, 128.0, "9.0"));

        assert_eq!(cluster.nodes().len(), 1);
        assert_eq!(cluster.nodes()[0].vram_gb, 48.0);
        assert_eq!(cluster.nodes()[0].system_memory_gb, 128.0);
    }
}
