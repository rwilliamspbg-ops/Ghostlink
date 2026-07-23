//! KV cache primitives for a single sequence within a single transformer layer.
//!
//! Stores one contiguous, pre-allocated buffer for all tokens' keys and one
//! for all values (shape: `max_tokens_per_seq * num_heads * hidden_dim_per_head`
//! floats each), instead of one small heap allocation per token. The previous
//! design called `resize_with` on a `Vec<KVCacheEntry>` where every entry
//! independently allocated its own `keys`/`values` `Vec<f32>` — for the
//! default 8192-token max sequence length, a full-length `initialize()`
//! performed up to 16,384 separate small heap allocations. A single upfront
//! allocation eliminates that, keeps a sequence's KV data in one contiguous
//! cache-friendly region, and enables `read_range` — a single batched read
//! over a token span, the shape attention actually needs, instead of N
//! separate per-token reads.
//!
//! Reads and writes are guarded by an `RwLock` rather than a `Mutex`: real
//! attention reads happen far more often than writes (every position reads
//! all previous positions' KV every decode step; only one write happens per
//! step), so letting reads proceed concurrently with each other matters.
//!
//! This module has no current caller in `runtime.rs` — Ghostlink delegates
//! actual model execution to an external inference engine (llama-server /
//! Ollama, see `ghost-link::native_engine`) rather than computing attention
//! itself. It stays self-contained as a ready-to-use primitive for a future
//! local execution path, so it can evolve without affecting the stable
//! public API exported from `lib.rs`.

use std::sync::{Arc, RwLock};

pub const DEFAULT_MAX_TOKENS_PER_SEQ: usize = 8192;
pub const DEFAULT_NUM_HEADS: usize = 4;
pub const DEFAULT_HIDDEN_DIM_PER_HEAD: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KVCacheConfig {
    pub max_tokens_per_seq: usize,
    pub num_heads: usize,
    pub hidden_dim_per_head: usize,
}

impl KVCacheConfig {
    /// Number of `f32` elements one token's keys (or values) occupy.
    pub const fn token_width(&self) -> usize {
        self.num_heads * self.hidden_dim_per_head
    }
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

/// Owned copy of one token's cached key/value vectors, returned by `read_kv`.
/// Small and fixed-size (`config.token_width()` floats each) — not a copy of
/// the whole cache.
#[derive(Clone, Debug, PartialEq)]
pub struct KVCacheEntry {
    pub keys: Vec<f32>,
    pub values: Vec<f32>,
}

struct KvCacheState {
    /// Empty until the first `initialize()` call; allocated exactly once
    /// after that, sized for `config.max_tokens_per_seq` tokens.
    keys: Vec<f32>,
    values: Vec<f32>,
    /// Number of tokens actually written so far (<= config.max_tokens_per_seq).
    current_len: usize,
}

/// Cheap to clone (an `Arc` around the shared, lock-guarded buffer) — matches
/// the shared-state pattern used elsewhere in this crate (e.g. `ClusterState`).
#[derive(Clone)]
pub struct LayerKvCache {
    pub config: KVCacheConfig,
    state: Arc<RwLock<KvCacheState>>,
}

impl std::fmt::Debug for LayerKvCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Deliberately does not print the backing buffers — they can be
        // megabytes (max_tokens_per_seq * token_width * 2 floats).
        f.debug_struct("LayerKvCache")
            .field("config", &self.config)
            .field("current_len", &self.len())
            .finish()
    }
}

impl Default for LayerKvCache {
    fn default() -> Self {
        Self::new(KVCacheConfig::default())
    }
}

impl LayerKvCache {
    /// Creates an empty cache. Does not allocate the backing buffer yet —
    /// `initialize()` performs the single upfront allocation, sized once for
    /// `config.max_tokens_per_seq`.
    pub fn new(config: KVCacheConfig) -> Self {
        Self {
            config,
            state: Arc::new(RwLock::new(KvCacheState {
                keys: Vec::new(),
                values: Vec::new(),
                current_len: 0,
            })),
        }
    }

    /// Ensures the backing buffer exists (allocating it on the first call
    /// only) and marks the first `seq_len` tokens (clamped to
    /// `config.max_tokens_per_seq`) as valid.
    ///
    /// Safe to call again on the same cache — e.g. to grow `current_len` as
    /// more tokens are generated — without losing previously written data:
    /// the buffer is allocated once and never reallocated by this method.
    pub fn initialize(&self, seq_len: usize) -> Result<(), String> {
        if seq_len == 0 {
            return Err("sequence length must be greater than zero".to_string());
        }

        let bounded_len = seq_len.min(self.config.max_tokens_per_seq);
        let width = self.config.token_width();
        let mut state = self
            .state
            .write()
            .map_err(|_| "kv cache lock poisoned".to_string())?;

        if state.keys.is_empty() {
            let capacity = self.config.max_tokens_per_seq * width;
            state.keys = vec![0.0; capacity];
            state.values = vec![0.0; capacity];
        }

        state.current_len = state.current_len.max(bounded_len);
        Ok(())
    }

    /// Writes one token's key/value vectors into the cache at `token_idx`.
    /// Copies directly into the pre-allocated buffer slice — no allocation.
    pub fn write_kv(&self, token_idx: usize, keys: &[f32], values: &[f32]) -> Result<(), String> {
        if keys.len() != values.len() {
            return Err("keys/values length mismatch".to_string());
        }
        let width = self.config.token_width();
        if keys.len() != width {
            return Err(format!(
                "expected {width} elements per token (num_heads * hidden_dim_per_head), got {}",
                keys.len()
            ));
        }
        if token_idx >= self.config.max_tokens_per_seq {
            return Err(format!(
                "token index {token_idx} out of bounds (max {})",
                self.config.max_tokens_per_seq
            ));
        }

        let mut state = self
            .state
            .write()
            .map_err(|_| "kv cache lock poisoned".to_string())?;
        if state.keys.is_empty() {
            return Err("cache not initialized — call initialize() first".to_string());
        }

        let start = token_idx * width;
        state.keys[start..start + width].copy_from_slice(keys);
        state.values[start..start + width].copy_from_slice(values);
        state.current_len = state.current_len.max(token_idx + 1);
        Ok(())
    }

    /// Reads one token's cached key/value vectors as an owned, fixed-size
    /// copy (`config.token_width()` floats each — not the whole cache).
    pub fn read_kv(&self, token_idx: usize) -> Result<KVCacheEntry, String> {
        let width = self.config.token_width();
        let state = self
            .state
            .read()
            .map_err(|_| "kv cache lock poisoned".to_string())?;

        if token_idx >= state.current_len {
            return Err(format!(
                "token index {token_idx} out of bounds (current length {})",
                state.current_len
            ));
        }

        let start = token_idx * width;
        Ok(KVCacheEntry {
            keys: state.keys[start..start + width].to_vec(),
            values: state.values[start..start + width].to_vec(),
        })
    }

    /// Reads keys/values for a contiguous token range (`[start_token,
    /// end_token)`) in one call — the shape attention actually wants (all
    /// prior positions at once), instead of `end_token - start_token`
    /// separate lock acquisitions and small allocations.
    pub fn read_range(
        &self,
        start_token: usize,
        end_token: usize,
    ) -> Result<(Vec<f32>, Vec<f32>), String> {
        if start_token > end_token {
            return Err("start_token must be <= end_token".to_string());
        }
        let width = self.config.token_width();
        let state = self
            .state
            .read()
            .map_err(|_| "kv cache lock poisoned".to_string())?;

        if end_token > state.current_len {
            return Err(format!(
                "end_token {end_token} exceeds current length {}",
                state.current_len
            ));
        }

        let start = start_token * width;
        let end = end_token * width;
        Ok((
            state.keys[start..end].to_vec(),
            state.values[start..end].to_vec(),
        ))
    }

    pub fn len(&self) -> usize {
        self.state.read().map(|s| s.current_len).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn small_config() -> KVCacheConfig {
        // Small width keeps test assertions easy to read; 2 heads * 4 dims = 8.
        KVCacheConfig {
            max_tokens_per_seq: 16,
            num_heads: 2,
            hidden_dim_per_head: 4,
        }
    }

    #[test]
    fn initialize_rejects_zero_len() {
        let cache = LayerKvCache::default();
        assert!(cache.initialize(0).is_err());
    }

    #[test]
    fn initialize_and_round_trip() {
        let cache = LayerKvCache::new(small_config());
        cache.initialize(8).expect("cache should initialize");
        assert_eq!(cache.len(), 8);

        let width = cache.config.token_width();
        let keys = vec![1.0_f32; width];
        let values = vec![2.0_f32; width];
        cache
            .write_kv(2, &keys, &values)
            .expect("write should succeed");

        let entry = cache.read_kv(2).expect("entry should exist");
        assert_eq!(entry.keys, keys);
        assert_eq!(entry.values, values);
    }

    #[test]
    fn growing_seq_len_preserves_prior_writes() {
        let cache = LayerKvCache::new(small_config());
        cache.initialize(4).unwrap();

        let width = cache.config.token_width();
        let keys = vec![7.0_f32; width];
        let values = vec![9.0_f32; width];
        cache.write_kv(1, &keys, &values).unwrap();

        // Growing current_len must not touch the already-allocated buffer,
        // so token 1's data must survive.
        cache.initialize(10).unwrap();
        assert_eq!(cache.len(), 10);

        let entry = cache.read_kv(1).unwrap();
        assert_eq!(entry.keys, keys);
        assert_eq!(entry.values, values);
    }

    #[test]
    fn write_kv_rejects_wrong_width() {
        let cache = LayerKvCache::new(small_config());
        cache.initialize(4).unwrap();
        let wrong_width = vec![0.0_f32; cache.config.token_width() + 1];
        assert!(cache.write_kv(0, &wrong_width, &wrong_width).is_err());
    }

    #[test]
    fn write_kv_rejects_out_of_bounds_token() {
        let cache = LayerKvCache::new(small_config());
        cache.initialize(4).unwrap();
        let width = cache.config.token_width();
        let data = vec![0.0_f32; width];
        assert!(cache.write_kv(999, &data, &data).is_err());
    }

    #[test]
    fn write_kv_before_initialize_is_rejected() {
        let cache = LayerKvCache::new(small_config());
        let width = cache.config.token_width();
        let data = vec![0.0_f32; width];
        assert!(cache.write_kv(0, &data, &data).is_err());
    }

    #[test]
    fn read_range_returns_contiguous_data() {
        let cache = LayerKvCache::new(small_config());
        cache.initialize(4).unwrap();
        let width = cache.config.token_width();

        for token_idx in 0..4 {
            let keys = vec![token_idx as f32; width];
            let values = vec![(token_idx * 10) as f32; width];
            cache.write_kv(token_idx, &keys, &values).unwrap();
        }

        let (keys, values) = cache.read_range(1, 3).unwrap();
        assert_eq!(keys.len(), 2 * width);
        assert_eq!(values.len(), 2 * width);
        // First token in the range is token_idx 1.
        assert_eq!(keys[0], 1.0);
        assert_eq!(values[0], 10.0);
        // Second token in the range is token_idx 2.
        assert_eq!(keys[width], 2.0);
        assert_eq!(values[width], 20.0);
    }

    #[test]
    fn read_range_rejects_end_beyond_current_len() {
        let cache = LayerKvCache::new(small_config());
        cache.initialize(4).unwrap();
        assert!(cache.read_range(0, 100).is_err());
    }

    #[test]
    fn is_empty_and_len_track_initialization() {
        let cache = LayerKvCache::new(small_config());
        assert!(cache.is_empty());
        cache.initialize(5).unwrap();
        assert!(!cache.is_empty());
        assert_eq!(cache.len(), 5);
    }

    #[test]
    fn concurrent_reads_do_not_corrupt_or_deadlock() {
        let cache = LayerKvCache::new(small_config());
        cache.initialize(8).unwrap();
        let width = cache.config.token_width();
        for token_idx in 0..8 {
            let v = vec![token_idx as f32; width];
            cache.write_kv(token_idx, &v, &v).unwrap();
        }

        let mut handles = Vec::new();
        for _ in 0..8 {
            let cache = cache.clone();
            handles.push(std::thread::spawn(move || {
                for token_idx in 0..8 {
                    let entry = cache.read_kv(token_idx).unwrap();
                    assert_eq!(entry.keys[0], token_idx as f32);
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
    }

    #[test]
    fn clone_shares_the_same_underlying_cache() {
        let cache = LayerKvCache::new(small_config());
        cache.initialize(4).unwrap();
        let width = cache.config.token_width();
        let data = vec![3.0_f32; width];

        let cache_clone = cache.clone();
        cache_clone.write_kv(0, &data, &data).unwrap();

        // Written through the clone, visible through the original — same
        // underlying Arc<RwLock<..>>, not a deep copy.
        assert_eq!(cache.read_kv(0).unwrap().keys, data);
    }
}
