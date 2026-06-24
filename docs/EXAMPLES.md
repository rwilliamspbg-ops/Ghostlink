# Ghost-Link Examples

## CLI

```bash
# Show CLI help
cargo run -p ghost-link -- help

# Generate an autotuned placement plan
cargo run -p ghost-link -- plan

# Emit a join frame
cargo run -p ghost-link -- join node-01

# Show the sample dashboard
cargo run -p ghost-link -- dashboard

# Fast cached probe mode
cargo run -p ghost-link -- probe workstation-a fast

# Full probe mode
cargo run -p ghost-link -- probe workstation-a full
```

## Testing

```bash
# Full workspace tests
cargo test --workspace

# Integration suite
cargo test -p ghostlink-core --test integration

# Criterion benchmarks
cargo bench -p ghostlink-core --bench criterion
```

## Rust API Example

```rust
use ghostlink_core::{
    detect_runtime_profile,
    planning::{assign_layers_with_runtime_profile, LayerSpec},
    protocol::NodeResources,
};

fn main() {
    let profile = detect_runtime_profile("node-a");

    let nodes = vec![
        NodeResources::new("node-a", 24.0, 64.0, "8.9", None),
        NodeResources::new("node-b", 12.0, 32.0, "8.6", None),
    ];

    let layers: Vec<LayerSpec> = (0..33)
        .map(|index| LayerSpec {
            index,
            vram_gb: 1.0,
            num_weights: 0,
        })
        .collect();

    let assignments = assign_layers_with_runtime_profile(&nodes, &layers, &profile).unwrap();
    println!("{} assignments", assignments.len());
}
```
