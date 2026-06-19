use crate::cluster::NodeResources;

const DELIVERY_RATIO_INT8_THRESHOLD: f32 = 0.95;
const DELIVERY_RATIO_INT4_THRESHOLD: f32 = 0.80;

#[derive(Clone, Debug, PartialEq)]
pub struct LayerSpec {
    pub index: usize,
    pub vram_gb: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LayerAssignment {
    pub node_id: String,
    pub start_layer: usize,
    pub end_layer: usize,
    pub used_vram_gb: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuantizationMode {
    None,
    Int8,
    Int4,
}

pub fn select_quantization_mode(delivery_ratio: f32) -> QuantizationMode {
    if delivery_ratio >= DELIVERY_RATIO_INT8_THRESHOLD {
        QuantizationMode::None
    } else if delivery_ratio >= DELIVERY_RATIO_INT4_THRESHOLD {
        QuantizationMode::Int8
    } else {
        QuantizationMode::Int4
    }
}

pub fn assign_layers_sequentially(
    nodes: &[NodeResources],
    layers: &[LayerSpec],
) -> Result<Vec<LayerAssignment>, String> {
    if nodes.is_empty() {
        return Err("at least one node is required".into());
    }
    if layers.is_empty() {
        return Ok(Vec::new());
    }

    let mut assignments = Vec::new();
    let mut current_node_index = 0usize;
    let mut remaining_capacity = nodes[0].vram_gb;
    let mut current_assignment: Option<LayerAssignment> = None;

    for layer in layers {
        while layer.vram_gb > remaining_capacity {
            if let Some(assignment) = current_assignment.take() {
                assignments.push(assignment);
            }

            current_node_index += 1;
            if current_node_index >= nodes.len() {
                return Err(format!(
                    "insufficient cluster VRAM for layer {} (needs {:.2} GB)",
                    layer.index, layer.vram_gb
                ));
            }
            remaining_capacity = nodes[current_node_index].vram_gb;
        }

        remaining_capacity -= layer.vram_gb;
        match current_assignment.as_mut() {
            Some(assignment) => {
                assignment.end_layer = layer.index;
                assignment.used_vram_gb += layer.vram_gb;
            }
            None => {
                current_assignment = Some(LayerAssignment {
                    node_id: nodes[current_node_index].id.clone(),
                    start_layer: layer.index,
                    end_layer: layer.index,
                    used_vram_gb: layer.vram_gb,
                });
            }
        }
    }

    if let Some(assignment) = current_assignment {
        assignments.push(assignment);
    }

    Ok(assignments)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_layers(count: usize, vram_gb: f32) -> Vec<LayerSpec> {
        (0..count)
            .map(|index| LayerSpec { index, vram_gb })
            .collect()
    }

    #[test]
    fn greedily_places_layers_across_nodes() {
        let nodes = vec![
            NodeResources::new("node-a", 24.0, 64.0, "8.9"),
            NodeResources::new("node-b", 12.0, 32.0, "8.6"),
        ];

        let assignments = assign_layers_sequentially(&nodes, &sample_layers(33, 1.0)).unwrap();

        assert_eq!(
            assignments,
            vec![
                LayerAssignment {
                    node_id: "node-a".into(),
                    start_layer: 0,
                    end_layer: 23,
                    used_vram_gb: 24.0,
                },
                LayerAssignment {
                    node_id: "node-b".into(),
                    start_layer: 24,
                    end_layer: 32,
                    used_vram_gb: 9.0,
                }
            ]
        );
    }

    #[test]
    fn reports_insufficient_capacity() {
        let nodes = vec![NodeResources::new("node-a", 2.0, 64.0, "8.9")];
        let error = assign_layers_sequentially(&nodes, &sample_layers(3, 1.0)).unwrap_err();

        assert!(error.contains("insufficient cluster VRAM"));
    }

    #[test]
    fn selects_quantization_mode_from_delivery_ratio() {
        assert_eq!(select_quantization_mode(0.98), QuantizationMode::None);
        assert_eq!(select_quantization_mode(0.90), QuantizationMode::Int8);
        assert_eq!(select_quantization_mode(0.75), QuantizationMode::Int4);
    }
}
