//! N3 LRU cache for validation results.
//!
//! Implements a simple LRU (Least Recently Used) cache using
//! `std::collections::HashMap` and a `Vec` access-order tracker. No external
//! dependency required.
//!
//! The cache maps validated action keys to [`N3Result`] values, avoiding
//! redundant LLM calls for identical actions within the same session.

use std::collections::HashMap;

/// A simple LRU cache with a configurable maximum size.
///
/// Evicts the least recently accessed entry when the cache exceeds
/// `max_size`. Thread-safety is provided by the caller (see
/// [`super::GuardianN3`] which wraps this in a `parking_lot::Mutex`).
#[derive(Debug)]
pub struct LruCache<V> {
    max_size: usize,
    entries: HashMap<String, CacheEntry<V>>,
    /// Access-order tracking: keys ordered from least recently used (front)
    /// to most recently used (back).
    order: Vec<String>,
}

#[derive(Debug, Clone)]
struct CacheEntry<V> {
    value: V,
}

impl<V: Clone> LruCache<V> {
    /// Create a new LRU cache with the given maximum size.
    ///
    /// # Panics
    ///
    /// Panics if `max_size` is 0 (cache would be useless).
    pub fn new(max_size: usize) -> Self {
        assert!(max_size > 0, "LruCache max_size must be > 0");
        Self {
            max_size,
            entries: HashMap::with_capacity(max_size),
            order: Vec::with_capacity(max_size),
        }
    }

    /// Get a value from the cache by key, promoting it to most recently used.
    pub fn get(&mut self, key: &str) -> Option<V> {
        if !self.entries.contains_key(key) {
            return None;
        }
        // Promote: move key to end of order list.
        if let Some(pos) = self.order.iter().position(|k| k == key) {
            self.order.remove(pos);
            self.order.push(key.to_string());
        }
        self.entries.get(key).map(|entry| entry.value.clone())
    }

    /// Insert a value into the cache.
    ///
    /// If the key already exists, its value is updated and promoted to
    /// most recently used. If the cache is full, the least recently used
    /// entry is evicted.
    pub fn insert(&mut self, key: String, value: V) {
        if self.entries.contains_key(&key) {
            // Update existing entry and promote.
            self.entries.insert(key.clone(), CacheEntry { value });
            if let Some(pos) = self.order.iter().position(|k| *k == key) {
                self.order.remove(pos);
            }
            self.order.push(key);
            return;
        }

        // Evict LRU entry if at capacity.
        if self.entries.len() >= self.max_size {
            if let Some(lru_key) = self.order.first().cloned() {
                self.entries.remove(&lru_key);
                self.order.remove(0);
            }
        }

        // Insert new entry.
        self.entries.insert(key.clone(), CacheEntry { value });
        self.order.push(key);
    }

    /// Remove a specific key from the cache.
    pub fn remove(&mut self, key: &str) -> Option<V> {
        if let Some(pos) = self.order.iter().position(|k| k == key) {
            self.order.remove(pos);
        }
        self.entries.remove(key).map(|entry| entry.value)
    }

    /// Clear all entries from the cache.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
    }

    /// Return the current number of entries in the cache.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Return true if the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Return the maximum capacity of the cache.
    pub fn max_size(&self) -> usize {
        self.max_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Basic operations
    // -----------------------------------------------------------------------

    #[test]
    fn cache_stores_and_retrieves_values() {
        let mut cache = LruCache::new(10);
        cache.insert("key1".into(), 42);
        cache.insert("key2".into(), 99);

        assert_eq!(cache.get("key1"), Some(42));
        assert_eq!(cache.get("key2"), Some(99));
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn cache_returns_none_for_missing_key() {
        let mut cache: LruCache<i32> = LruCache::new(10);
        assert_eq!(cache.get("nonexistent"), None);
    }

    #[test]
    fn cache_overwrites_existing_key() {
        let mut cache = LruCache::new(10);
        cache.insert("key1".into(), 42);
        cache.insert("key1".into(), 100);

        assert_eq!(cache.get("key1"), Some(100));
        assert_eq!(cache.len(), 1);
    }

    // -----------------------------------------------------------------------
    // LRU eviction
    // -----------------------------------------------------------------------

    #[test]
    fn cache_evicts_lru_when_full() {
        let mut cache = LruCache::new(3);
        cache.insert("a".into(), 1);
        cache.insert("b".into(), 2);
        cache.insert("c".into(), 3);

        // Cache is full. Inserting 'd' should evict 'a' (LRU).
        cache.insert("d".into(), 4);

        assert_eq!(cache.len(), 3);
        assert_eq!(cache.get("a"), None, "LRU entry 'a' should be evicted");
        assert_eq!(cache.get("b"), Some(2));
        assert_eq!(cache.get("c"), Some(3));
        assert_eq!(cache.get("d"), Some(4));
    }

    #[test]
    fn cache_get_promotes_entry() {
        let mut cache = LruCache::new(3);
        cache.insert("a".into(), 1);
        cache.insert("b".into(), 2);
        cache.insert("c".into(), 3);

        // Access 'a' — it moves to most recently used.
        assert_eq!(cache.get("a"), Some(1));

        // Now 'b' is LRU. Inserting 'd' should evict 'b'.
        cache.insert("d".into(), 4);

        assert_eq!(cache.get("a"), Some(1), "'a' was promoted and should remain");
        assert_eq!(cache.get("b"), None, "'b' was LRU and should be evicted");
        assert_eq!(cache.get("c"), Some(3));
        assert_eq!(cache.get("d"), Some(4));
    }

    #[test]
    fn cache_evicts_in_access_order() {
        let mut cache = LruCache::new(2);
        cache.insert("a".into(), 1);
        cache.insert("b".into(), 2);
        assert_eq!(cache.len(), 2);

        cache.insert("c".into(), 3);
        assert_eq!(cache.get("a"), None, "'a' should be evicted");

        cache.insert("d".into(), 4);
        assert_eq!(cache.get("b"), None, "'b' should be evicted");
    }

    // -----------------------------------------------------------------------
    // Remove and clear
    // -----------------------------------------------------------------------

    #[test]
    fn cache_remove_removes_entry() {
        let mut cache = LruCache::new(10);
        cache.insert("key1".into(), 42);
        assert_eq!(cache.remove("key1"), Some(42));
        assert_eq!(cache.get("key1"), None);
        assert!(cache.is_empty());
    }

    #[test]
    fn cache_clear_empties_all() {
        let mut cache = LruCache::new(10);
        cache.insert("a".into(), 1);
        cache.insert("b".into(), 2);
        cache.insert("c".into(), 3);

        cache.clear();
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
    }

    // -----------------------------------------------------------------------
    // Edge cases
    // -----------------------------------------------------------------------

    #[test]
    fn cache_handles_single_entry() {
        let mut cache = LruCache::new(1);
        cache.insert("only".into(), 1);
        assert_eq!(cache.get("only"), Some(1));

        // Inserting another should evict 'only'.
        cache.insert("new".into(), 2);
        assert_eq!(cache.get("only"), None);
        assert_eq!(cache.get("new"), Some(2));
    }

    #[test]
    #[should_panic(expected = "max_size must be > 0")]
    fn cache_rejects_zero_size() {
        let _cache: LruCache<i32> = LruCache::new(0);
    }

    #[test]
    fn cache_insert_without_eviction_maintains_order() {
        let mut cache = LruCache::new(5);
        cache.insert("a".into(), 1);
        cache.insert("b".into(), 2);
        cache.insert("c".into(), 3);

        assert_eq!(cache.len(), 3);
        assert_eq!(cache.get("a"), Some(1));
        assert_eq!(cache.get("b"), Some(2));
        assert_eq!(cache.get("c"), Some(3));
    }

    // -----------------------------------------------------------------------
    // Clone values (the cache returns cloned values)
    // -----------------------------------------------------------------------

    #[test]
    fn cache_returns_clone_not_reference() {
        let mut cache = LruCache::new(10);
        cache.insert("key".into(), vec![1, 2, 3]);

        let val1 = cache.get("key").unwrap();
        let val2 = cache.get("key").unwrap();
        assert_eq!(val1, val2);

        // They should be independent clones.
        drop(val1);
        assert_eq!(cache.get("key"), Some(vec![1, 2, 3]));
    }
}
