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

# Validate GUI readiness and emit categorized diagnostics
cargo run -p ghost-link -- gui-check --strict
GHOSTLINK_GUI_DIAG_JSON=./tmp/gui-diag.json cargo run -p ghost-link -- gui-diagnose --strict

# Fast cached probe mode
cargo run -p ghost-link -- probe workstation-a fast

# Full probe mode
cargo run -p ghost-link -- probe workstation-a full

# Use TOML config defaults for flow
cargo run -p ghost-link -- --config ./ghostlink.example.toml flow

# Use env-based config file fallback
GHOSTLINK_CONFIG=./ghostlink.example.toml cargo run -p ghost-link -- cluster-start
```

## Testing

```bash
# Full workspace tests
cargo test --workspace

# Integration suite
cargo test -p ghostlink-core --test integration

# Criterion benchmarks
cargo bench -p ghostlink-core --bench criterion

# Validate GUI endpoint contract drift against mock backend
python3 scripts/validate_gui_api_contract.py
```

## Model Download Verification

```bash
# Install the Hugging Face hub client once
python3 -m pip install huggingface_hub

# Verify default tiny repos and config.json download
python3 scripts/verify_hf_models.py

# Verify specific repos/files
python3 scripts/verify_hf_models.py --repo mistralai/Mistral-7B-v0.1 --file config.json

# Optional: use an auth token for higher rate limits
export HF_TOKEN=your_token_here
python3 scripts/verify_hf_models.py --repo meta-llama/Llama-3.2-1B --file config.json
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
