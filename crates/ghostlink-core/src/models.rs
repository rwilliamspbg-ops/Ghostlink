//! Model Registry and Asset Management
//!
//! This module tracks locally available model assets, their metadata,
//! and provides hooks for loading/unloading from accelerators.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Metadata for a locally stored model.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ModelMetadata {
    pub id: String,
    pub name: String,
    pub architecture: String,
    pub total_layers: usize,
    pub parameter_count: u64,
    pub precision: String,
    pub vram_required_gb: f32,
    pub local_path: PathBuf,
}

/// Status of a model in the registry.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ModelStatus {
    Downloaded,
    Loading,
    Loaded,
    Unloading,
}

/// A registry of model assets available on the local node.
pub struct ModelRegistry {
    models: Arc<Mutex<HashMap<String, ModelMetadata>>>,
    status: Arc<Mutex<HashMap<String, ModelStatus>>>,
    base_dir: PathBuf,
}

impl ModelRegistry {
    /// Create a new model registry using the specified base directory.
    pub fn new<P: AsRef<Path>>(base_dir: P) -> Self {
        Self {
            models: Arc::new(Mutex::new(HashMap::new())),
            status: Arc::new(Mutex::new(HashMap::new())),
            base_dir: base_dir.as_ref().to_path_buf(),
        }
    }

    /// Register a model in the registry.
    pub fn register(&self, metadata: ModelMetadata) {
        let mut models = self.models.lock().unwrap();
        models.insert(metadata.id.clone(), metadata);
    }

    /// Get metadata for a specific model.
    pub fn get_metadata(&self, id: &str) -> Option<ModelMetadata> {
        let models = self.models.lock().unwrap();
        models.get(id).cloned()
    }

    /// List all models in the registry.
    pub fn list_models(&self) -> Vec<ModelMetadata> {
        let models = self.models.lock().unwrap();
        models.values().cloned().collect()
    }

    /// Set the status of a model.
    pub fn set_status(&self, id: &str, status: ModelStatus) {
        let mut statuses = self.status.lock().unwrap();
        statuses.insert(id.to_string(), status);
    }

    /// Get the status of a model.
    pub fn get_status(&self, id: &str) -> ModelStatus {
        let statuses = self.status.lock().unwrap();
        statuses.get(id).copied().unwrap_or(ModelStatus::Downloaded)
    }

    /// Return the local storage base directory.
    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_model_registration() {
        let dir = tempdir().unwrap();
        let registry = ModelRegistry::new(dir.path());
        let meta = ModelMetadata {
            id: "llama3-8b".to_string(),
            name: "Llama 3 8B".to_string(),
            architecture: "llama".to_string(),
            total_layers: 32,
            parameter_count: 8_000_000_000,
            precision: "f16".to_string(),
            vram_required_gb: 16.0,
            local_path: dir.path().join("llama3-8b"),
        };

        registry.register(meta.clone());
        assert_eq!(registry.get_metadata("llama3-8b"), Some(meta));
        assert_eq!(registry.get_status("llama3-8b"), ModelStatus::Downloaded);

        registry.set_status("llama3-8b", ModelStatus::Loaded);
        assert_eq!(registry.get_status("llama3-8b"), ModelStatus::Loaded);
    }
}
