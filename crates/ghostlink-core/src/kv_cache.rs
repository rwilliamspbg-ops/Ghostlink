//! KV cache primitives used by staged execution experiments.
//!
//! This module intentionally stays self-contained so it can evolve without
//! affecting the stable public API exported from `lib.rs`.

use std::sync::{Arc, Mutex};

pub const DEFAULT_MAX_TOKENS_PER_SEQ: usize = 8192;
pub const DEFAULT_NUM_HEADS: usize = 4;
pub const DEFAULT_HIDDEN_DIM_PER_HEAD: usize = 64;

#[derive(Clone, Copy, Debug)]
pub struct KVCacheConfig {
    pub max_tokens_per_seq: usize,
    pub num_heads: usize,
    pub hidden_dim_per_head: usize,
}

impl Default for KVCacheConfig {
    fn default() -> Self {
        Self {
            max_tokens_per_seq: DEFAULT_MAX_TOKENS_PER_SEQ,
            num_heads: DEFAULT_NUM_HEADS,
            hidden_dim_per_head: DEFAULT_HIDDEN_DIM_PER_HEAD,
        }
    }
}

#[derive(Clone, Debug)]
pub struct KVCacheEntry {
    pub keys: Vec<f32>,
    pub values: Vec<f32>,
    pub current_len: usize,
}

impl KVCacheEntry {
    fn with_shape(config: KVCacheConfig, seq_len: usize) -> Self {
        let shape = config.num_heads * config.hidden_dim_per_head;
        Self {
            keys: vec![0.0; shape],
            values: vec![0.0; shape],
            current_len: seq_len,
        }
    }
}

#[derive(Clone, Debug)]
pub struct LayerKvCache {
    pub config: KVCacheConfig,
    entries: Arc<Mutex<Vec<KVCacheEntry>>>,
}

impl Default for LayerKvCache {
    fn default() -> Self {
        Self {
            config: KVCacheConfig::default(),
            entries: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl LayerKvCache {
    pub fn initialize(&mut self, seq_len: usize) -> Result<(), String> {
        if seq_len == 0 {
            return Err("sequence length must be greater than zero".to_string());
        }

        let bounded_len = seq_len.min(self.config.max_tokens_per_seq);
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| "kv cache mutex poisoned".to_string())?;

        if entries.len() < bounded_len {
            entries.resize_with(bounded_len, || {
                KVCacheEntry::with_shape(self.config, bounded_len)
            });
        }

        for entry in entries.iter_mut().take(bounded_len) {
            entry.current_len = bounded_len;
        }

        entries.truncate(bounded_len);
        Ok(())
    }

    pub fn write_kv(
        &mut self,
        token_idx: usize,
        keys: &[f32],
        values: &[f32],
    ) -> Result<(), String> {
        if keys.len() != values.len() {
            return Err("keys/values length mismatch".to_string());
        }

        let mut entries = self
            .entries
            .lock()
            .map_err(|_| "kv cache mutex poisoned".to_string())?;

        let entry = entries
            .get_mut(token_idx)
            .ok_or_else(|| format!("token index {} out of bounds", token_idx))?;

        if entry.keys.len() != keys.len() {
            return Err("incoming KV shape does not match cache entry shape".to_string());
        }

        entry.keys.copy_from_slice(keys);
        entry.values.copy_from_slice(values);
        Ok(())
    }

    pub fn read_kv(&self, token_idx: usize) -> Result<KVCacheEntry, String> {
        let entries = self
            .entries
            .lock()
            .map_err(|_| "kv cache mutex poisoned".to_string())?;

        entries
            .get(token_idx)
            .cloned()
            .ok_or_else(|| format!("token index {} out of bounds", token_idx))
    }

    pub fn len(&self) -> usize {
        self.entries.lock().map(|v| v.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initialize_rejects_zero_len() {
        let mut cache = LayerKvCache::default();
        assert!(cache.initialize(0).is_err());
    }

    #[test]
    fn initialize_and_round_trip() {
        let mut cache = LayerKvCache::default();
        cache.initialize(8).expect("cache should initialize");

        let width = cache.config.num_heads * cache.config.hidden_dim_per_head;
        let keys = vec![1.0_f32; width];
        let values = vec![2.0_f32; width];
        cache
            .write_kv(2, &keys, &values)
            .expect("write should succeed");

        let entry = cache.read_kv(2).expect("entry should exist");
        assert_eq!(entry.current_len, 8);
        assert_eq!(entry.keys[0], 1.0);
        assert_eq!(entry.values[0], 2.0);
    }
}
