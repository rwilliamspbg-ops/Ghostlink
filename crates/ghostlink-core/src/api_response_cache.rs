//! API Response Caching Layer with ETag Support
//!
//! Reduces serialization overhead and bandwidth by caching API responses with:
//! - TTL-based expiration
//! - ETag support for conditional requests (304 Not Modified)
//! - Hit ratio tracking and statistics
//! - Optional pre-warming on startup

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

/// Cached API response
#[derive(Clone, Debug)]
struct CachedResponse {
    /// Response body - optimized to Arc<[u8]> to allow cheap, O(1), zero-allocation clones on cache hits
    body: Arc<[u8]>,
    /// ETag hash
    etag: String,
    /// Expiration time
    expires_at: Instant,
}

/// Cache statistics for observability
#[derive(Clone, Debug, Default)]
pub struct CacheStats {
    pub total_requests: usize,
    pub cache_hits: usize,
    pub cache_misses: usize,
    pub etag_hits: usize,
}

impl CacheStats {
    pub fn hit_ratio(&self) -> f64 {
        if self.total_requests == 0 {
            0.0
        } else {
            (self.cache_hits + self.etag_hits) as f64 / self.total_requests as f64
        }
    }
}

/// Thread-safe API response cache
#[derive(Debug)]
pub struct ApiResponseCache {
    cache: Arc<RwLock<HashMap<String, CachedResponse>>>,
    stats: Arc<RwLock<CacheStats>>,
}

impl ApiResponseCache {
    /// Create a new cache
    pub fn new() -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(CacheStats::default())),
        }
    }

    /// Get cached response or compute new one.
    ///
    /// Optimized to return Arc<[u8]> instead of Vec<u8> to prevent cloning the entire
    /// response body (which can be multi-megabyte JSON) on cache hits. This reduces allocation churn
    /// and memory traffic from O(N) to O(1) time and space complexity.
    pub fn get_or_compute<F>(
        &self,
        key: &str,
        ttl: Duration,
        compute_fn: F,
    ) -> Result<(Arc<[u8]>, String), String>
    where
        F: FnOnce() -> Result<Vec<u8>, String>,
    {
        let mut stats = self.stats.write().unwrap();
        stats.total_requests += 1;

        // Check cache
        {
            let cache = self.cache.read().unwrap();
            if let Some(cached) = cache.get(key) {
                if cached.expires_at > Instant::now() {
                    stats.cache_hits += 1;
                    return Ok((Arc::clone(&cached.body), cached.etag.clone()));
                }
            }
        }

        // Cache miss or expired
        stats.cache_misses += 1;
        let body = compute_fn()?;
        let etag = compute_etag(&body);
        let body_arc: Arc<[u8]> = Arc::from(body);

        let mut cache = self.cache.write().unwrap();
        cache.insert(
            key.to_string(),
            CachedResponse {
                body: Arc::clone(&body_arc),
                etag: etag.clone(),
                expires_at: Instant::now() + ttl,
            },
        );

        Ok((body_arc, etag))
    }

    /// Check if client's ETag matches (for 304 responses)
    pub fn etag_matches(&self, key: &str, client_etag: &str) -> bool {
        let cache = self.cache.read().unwrap();
        if let Some(cached) = cache.get(key) {
            if cached.expires_at > Instant::now() {
                return cached.etag == client_etag;
            }
        }
        false
    }

    /// Record ETag hit (304 response sent)
    pub fn record_etag_hit(&self) {
        let mut stats = self.stats.write().unwrap();
        stats.etag_hits += 1;
    }

    /// Get current cache statistics
    pub fn stats(&self) -> CacheStats {
        self.stats.read().unwrap().clone()
    }

    /// Clear all cache entries
    pub fn clear(&self) {
        self.cache.write().unwrap().clear();
        *self.stats.write().unwrap() = CacheStats::default();
    }

    /// Pre-warm cache with known endpoints
    pub fn pre_warm<F>(&self, endpoints: Vec<(&str, Duration)>, compute_fn: F) -> Result<(), String>
    where
        F: Fn(&str) -> Result<Vec<u8>, String>,
    {
        for (key, ttl) in endpoints {
            self.get_or_compute(key, ttl, || compute_fn(key))?;
        }
        Ok(())
    }

    /// Get cache size (number of entries)
    pub fn size(&self) -> usize {
        self.cache.read().unwrap().len()
    }
}

impl Default for ApiResponseCache {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for ApiResponseCache {
    fn clone(&self) -> Self {
        Self {
            cache: Arc::clone(&self.cache),
            stats: Arc::clone(&self.stats),
        }
    }
}

/// Compute a weak ETag (RFC 7232 `W/` prefix — a content fingerprint for
/// conditional-GET purposes, not a claim of byte-exact equivalence) from a
/// response body. Mixes the body length into the hash state and appends it
/// to the digest so two different-length bodies that happen to collide in
/// the 64-bit hash are still distinguishable — plain `DefaultHasher::finish()`
/// alone shares this any-fixed-width-hash's theoretical collision risk, but
/// unlike a 32-bit CRC (already used elsewhere in this crate for wire-frame
/// corruption detection, a different threat model) this doesn't halve the
/// effective space to make it worse.
fn compute_etag(body: &[u8]) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    body.len().hash(&mut hasher);
    body.hash(&mut hasher);
    format!("W/\"{:x}-{:x}\"", hasher.finish(), body.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_miss_computes_value() {
        let cache = ApiResponseCache::new();
        let body = b"response data".to_vec();
        let body_clone = body.clone();

        let (result, etag) = cache
            .get_or_compute("test", Duration::from_secs(60), || Ok(body_clone))
            .unwrap();

        assert_eq!(result.as_ref(), body.as_slice());
        assert!(!etag.is_empty());
    }

    #[test]
    fn cache_hit_reuses() {
        let cache = ApiResponseCache::new();
        let body1 = b"response1".to_vec();
        let body1_clone = body1.clone();

        let (result1, etag1) = cache
            .get_or_compute("test", Duration::from_secs(60), || Ok(body1_clone))
            .unwrap();

        let (result2, etag2) = cache
            .get_or_compute(
                "test",
                Duration::from_secs(60),
                || Ok(b"response2".to_vec()),
            )
            .unwrap();

        assert_eq!(result1, result2);
        assert_eq!(etag1, etag2);
        assert_eq!(cache.stats().cache_hits, 1);
    }

    #[test]
    fn etag_match_detection() {
        let cache = ApiResponseCache::new();
        let body = b"data".to_vec();
        let body_clone = body.clone();

        let (_result, etag) = cache
            .get_or_compute("test", Duration::from_secs(60), || Ok(body_clone))
            .unwrap();

        assert!(cache.etag_matches("test", &etag));
        assert!(!cache.etag_matches("test", "wrong-etag"));
    }

    #[test]
    fn expiration_and_recompute() {
        let cache = ApiResponseCache::new();

        // First request: cache miss
        cache
            .get_or_compute("test", Duration::from_millis(5), || Ok(b"data1".to_vec()))
            .unwrap();

        // Wait for expiration
        std::thread::sleep(Duration::from_millis(50));

        // Second request on expired key: should recompute
        let (result2, _etag2) = cache
            .get_or_compute("test", Duration::from_secs(60), || Ok(b"data2".to_vec()))
            .unwrap();

        assert_eq!(result2.as_ref(), b"data2");
    }

    #[test]
    fn statistics_tracking() {
        let cache = ApiResponseCache::new();

        for i in 0..5 {
            let body = format!("response{}", i).into_bytes();
            cache
                .get_or_compute(&format!("endpoint{}", i), Duration::from_secs(60), || {
                    Ok(body)
                })
                .unwrap();
        }

        let stats = cache.stats();
        assert_eq!(stats.total_requests, 5);
        assert_eq!(stats.cache_misses, 5);
        assert_eq!(stats.cache_hits, 0);
    }

    #[test]
    fn hit_ratio_calculation() {
        let cache = ApiResponseCache::new();

        for _ in 0..3 {
            cache
                .get_or_compute("test", Duration::from_secs(60), || Ok(b"data".to_vec()))
                .unwrap();
            cache
                .get_or_compute("test", Duration::from_secs(60), || Ok(b"data".to_vec()))
                .unwrap();
        }

        let stats = cache.stats();
        let ratio = stats.hit_ratio();
        assert!(ratio > 0.5);
    }

    #[test]
    fn pre_warm_populates() {
        let cache = ApiResponseCache::new();

        cache
            .pre_warm(
                vec![
                    ("models", Duration::from_secs(60)),
                    ("runtime", Duration::from_secs(300)),
                ],
                |_key| Ok(b"warm data".to_vec()),
            )
            .unwrap();

        assert_eq!(cache.size(), 2);
    }

    #[test]
    fn clone_shares_state() {
        let cache1 = ApiResponseCache::new();
        cache1
            .get_or_compute("test", Duration::from_secs(60), || Ok(b"data".to_vec()))
            .unwrap();

        let cache2 = cache1.clone();
        assert_eq!(cache1.size(), cache2.size());
    }
}
