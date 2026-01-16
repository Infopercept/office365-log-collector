//! Memory-efficient known blobs cache with LRU eviction and TTL support.
//!
//! This module addresses the memory leak issue where known_blobs HashMap would grow
//! unboundedly during daemon mode operation. It uses:
//! - LRU (Least Recently Used) eviction to cap maximum entries
//! - TTL (Time-To-Live) based expiration using blob expiration times
//! - Periodic cleanup of expired entries during runtime

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, BufRead, LineWriter, Write};
use std::path::Path;
use std::sync::Arc;
use chrono::{DateTime, NaiveDateTime, Utc};
use log::{debug, info, warn};
use lru::LruCache;
use std::num::NonZeroUsize;
use tokio::sync::RwLock;

/// Maximum number of blob IDs to keep in memory.
/// Office365 content blobs expire after 24 hours, and at typical ingestion rates
/// of ~100k-500k blobs per day, 1M entries provides comfortable headroom.
const DEFAULT_MAX_ENTRIES: usize = 1_000_000;

/// How often to run expiration cleanup (in number of inserts)
const CLEANUP_INTERVAL: usize = 10_000;

/// Thread-safe LRU cache for known blob IDs with TTL-based expiration.
///
/// This replaces the unbounded HashMap<String, String> that was causing
/// memory leaks during long-running daemon mode.
pub struct KnownBlobsCache {
    /// LRU cache storing blob_id -> expiration_time
    cache: LruCache<String, DateTime<Utc>>,
    /// Counter for periodic cleanup
    insert_count: usize,
    /// Maximum entries allowed
    max_entries: usize,
}

impl KnownBlobsCache {
    /// Create a new cache with default capacity
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_MAX_ENTRIES)
    }

    /// Create a new cache with specified maximum capacity
    pub fn with_capacity(max_entries: usize) -> Self {
        let cap = NonZeroUsize::new(max_entries).unwrap_or(NonZeroUsize::new(1).unwrap());
        KnownBlobsCache {
            cache: LruCache::new(cap),
            insert_count: 0,
            max_entries,
        }
    }

    /// Check if a blob ID is known (not expired)
    pub fn contains(&mut self, blob_id: &str) -> bool {
        if let Some(expiration) = self.cache.get(blob_id) {
            // Check if expired
            if Utc::now() >= *expiration {
                // Remove expired entry
                self.cache.pop(blob_id);
                return false;
            }
            return true;
        }
        false
    }

    /// Insert a blob ID with its expiration time
    pub fn insert(&mut self, blob_id: String, expiration_str: &str) {
        // Parse expiration time
        let expiration = parse_expiration(expiration_str);

        // Only insert if not already expired
        if let Some(exp_time) = expiration {
            if Utc::now() < exp_time {
                self.cache.put(blob_id, exp_time);
                self.insert_count += 1;

                // Periodic cleanup of expired entries
                if self.insert_count >= CLEANUP_INTERVAL {
                    self.cleanup_expired();
                    self.insert_count = 0;
                }
            }
        }
    }

    /// Remove all expired entries from the cache
    pub fn cleanup_expired(&mut self) {
        let now = Utc::now();
        let mut expired_keys: Vec<String> = Vec::new();

        // Collect expired keys (we need to iterate without holding mutable borrow)
        for (key, expiration) in self.cache.iter() {
            if now >= *expiration {
                expired_keys.push(key.clone());
            }
        }

        let expired_count = expired_keys.len();

        // Remove expired entries
        for key in expired_keys {
            self.cache.pop(&key);
        }

        if expired_count > 0 {
            debug!("Cleaned up {} expired blob entries, {} remaining",
                   expired_count, self.cache.len());
        }
    }

    /// Get the current number of entries
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }

    /// Load known blobs from file, filtering out expired entries
    pub fn load_from_file(path: &Path) -> Self {
        let mut cache = Self::new();

        if !path.exists() {
            info!("No existing known_blobs file, starting fresh");
            return cache;
        }

        let file = match File::open(path) {
            Ok(f) => f,
            Err(e) => {
                warn!("Could not open known_blobs file: {}", e);
                return cache;
            }
        };

        let reader = BufReader::new(file);
        let now = Utc::now();
        let mut loaded = 0;
        let mut skipped_expired = 0;
        let mut skipped_invalid = 0;

        for line in reader.lines() {
            let line = match line {
                Ok(l) => l,
                Err(_) => continue,
            };

            if line.trim().is_empty() {
                continue;
            }

            if let Some((id, expiration_str)) = line.split_once(',') {
                if let Some(expiration) = parse_expiration(expiration_str.trim()) {
                    if now < expiration {
                        // Not expired, add to cache
                        cache.cache.put(id.trim().to_string(), expiration);
                        loaded += 1;
                    } else {
                        skipped_expired += 1;
                    }
                } else {
                    skipped_invalid += 1;
                }
            } else {
                skipped_invalid += 1;
            }
        }

        info!("Loaded {} known blobs, skipped {} expired, {} invalid",
              loaded, skipped_expired, skipped_invalid);

        cache
    }

    /// Save cache to file
    pub fn save_to_file(&mut self, path: &Path) -> std::io::Result<()> {
        // Clean up expired before saving
        self.cleanup_expired();

        let file = File::create(path)?;
        let mut writer = LineWriter::new(file);

        for (id, expiration) in self.cache.iter() {
            let expiration_str = expiration.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
            writeln!(writer, "{},{}", id, expiration_str)?;
        }

        writer.flush()?;
        info!("Saved {} known blobs to file", self.cache.len());
        Ok(())
    }

    /// Convert from legacy HashMap format
    pub fn from_hashmap(map: HashMap<String, String>) -> Self {
        let mut cache = Self::with_capacity(map.len().max(DEFAULT_MAX_ENTRIES));
        let now = Utc::now();

        for (id, expiration_str) in map {
            if let Some(expiration) = parse_expiration(&expiration_str) {
                if now < expiration {
                    cache.cache.put(id, expiration);
                }
            }
        }

        cache
    }

    /// Convert to HashMap for compatibility with existing code
    pub fn to_hashmap(&self) -> HashMap<String, String> {
        let mut map = HashMap::new();
        for (id, expiration) in self.cache.iter() {
            let expiration_str = expiration.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
            map.insert(id.clone(), expiration_str);
        }
        map
    }
}

impl Default for KnownBlobsCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Thread-safe wrapper for shared access to KnownBlobsCache
pub struct SharedKnownBlobsCache {
    inner: Arc<RwLock<KnownBlobsCache>>,
}

impl SharedKnownBlobsCache {
    pub fn new() -> Self {
        SharedKnownBlobsCache {
            inner: Arc::new(RwLock::new(KnownBlobsCache::new())),
        }
    }

    pub fn from_cache(cache: KnownBlobsCache) -> Self {
        SharedKnownBlobsCache {
            inner: Arc::new(RwLock::new(cache)),
        }
    }

    pub async fn contains(&self, blob_id: &str) -> bool {
        let mut cache = self.inner.write().await;
        cache.contains(blob_id)
    }

    pub async fn insert(&self, blob_id: String, expiration: &str) {
        let mut cache = self.inner.write().await;
        cache.insert(blob_id, expiration);
    }

    pub async fn len(&self) -> usize {
        let cache = self.inner.read().await;
        cache.len()
    }

    pub async fn cleanup_expired(&self) {
        let mut cache = self.inner.write().await;
        cache.cleanup_expired();
    }

    pub async fn save_to_file(&self, path: &Path) -> std::io::Result<()> {
        let mut cache = self.inner.write().await;
        cache.save_to_file(path)
    }

    pub async fn to_hashmap(&self) -> HashMap<String, String> {
        let cache = self.inner.read().await;
        cache.to_hashmap()
    }

    pub fn clone_arc(&self) -> Self {
        SharedKnownBlobsCache {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl Clone for SharedKnownBlobsCache {
    fn clone(&self) -> Self {
        self.clone_arc()
    }
}

/// Parse expiration string into DateTime<Utc>
fn parse_expiration(s: &str) -> Option<DateTime<Utc>> {
    // Try multiple formats used by Office365 API
    let formats = [
        "%Y-%m-%dT%H:%M:%S%.fZ",
        "%Y-%m-%dT%H:%M:%SZ",
        "%Y-%m-%dT%H:%M:%S",
    ];

    for fmt in &formats {
        if let Ok(naive) = NaiveDateTime::parse_from_str(s.trim(), fmt) {
            return Some(DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc));
        }
    }

    // Try parsing with chrono's RFC3339
    if let Ok(dt) = DateTime::parse_from_rfc3339(s.trim()) {
        return Some(dt.with_timezone(&Utc));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_insert_and_contains() {
        let mut cache = KnownBlobsCache::new();

        // Insert with future expiration
        let future = (Utc::now() + chrono::Duration::hours(1))
            .format("%Y-%m-%dT%H:%M:%S%.3fZ")
            .to_string();

        cache.insert("test-blob-1".to_string(), &future);
        assert!(cache.contains("test-blob-1"));
        assert!(!cache.contains("nonexistent"));
    }

    #[test]
    fn test_cache_expired_not_inserted() {
        let mut cache = KnownBlobsCache::new();

        // Insert with past expiration
        let past = (Utc::now() - chrono::Duration::hours(1))
            .format("%Y-%m-%dT%H:%M:%S%.3fZ")
            .to_string();

        cache.insert("expired-blob".to_string(), &past);
        assert!(!cache.contains("expired-blob"));
    }

    #[test]
    fn test_cache_lru_eviction() {
        let mut cache = KnownBlobsCache::with_capacity(3);

        let future = (Utc::now() + chrono::Duration::hours(1))
            .format("%Y-%m-%dT%H:%M:%S%.3fZ")
            .to_string();

        cache.insert("blob-1".to_string(), &future);
        cache.insert("blob-2".to_string(), &future);
        cache.insert("blob-3".to_string(), &future);
        cache.insert("blob-4".to_string(), &future); // Should evict blob-1

        assert!(!cache.contains("blob-1")); // Evicted
        assert!(cache.contains("blob-2"));
        assert!(cache.contains("blob-3"));
        assert!(cache.contains("blob-4"));
    }

    #[test]
    fn test_parse_expiration() {
        assert!(parse_expiration("2030-01-01T00:00:00.000Z").is_some());
        assert!(parse_expiration("2030-01-01T00:00:00Z").is_some());
        assert!(parse_expiration("invalid").is_none());
    }
}
