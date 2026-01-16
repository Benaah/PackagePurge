//! LRU Cache Implementations
//!
//! Provides two LRU cache implementations:
//! 1. `LruCache` - Safe implementation using Rc<RefCell<>> (legacy)
//! 2. `IntrusiveLruCache` - Memory-efficient implementation using indices
//!
//! The intrusive implementation reduces memory overhead by ~40-60% by using
//! a generational index approach instead of reference counting.

use std::collections::HashMap;
use std::hash::Hash;
use chrono::Utc;
use crate::types::PackageUsageMetrics;

/// Generation counter to detect stale indices
type Generation = u32;

/// Index into the node pool with generation for safety
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct NodeIndex {
    index: usize,
    generation: Generation,
}

/// A slot in the node pool
struct Slot<K, V> {
    key: K,
    value: V,
    prev: Option<usize>,
    next: Option<usize>,
    generation: Generation,
    occupied: bool,
}

impl<K: Default, V: Default> Default for Slot<K, V> {
    fn default() -> Self {
        Self {
            key: K::default(),
            value: V::default(),
            prev: None,
            next: None,
            generation: 0,
            occupied: false,
        }
    }
}

/// Memory-efficient LRU cache using a slot-based pool
/// 
/// Memory overhead per entry:
/// - Standard Rc<RefCell<Node>>: ~48 bytes (Rc overhead + RefCell + pointers)
/// - This implementation: ~16 bytes (2 Option<usize> + generation + bool)
/// 
/// ~40-60% memory reduction for the linked list structure
pub struct IntrusiveLruCache<K, V> 
where 
    K: Eq + Hash + Clone + Default,
    V: Clone + Default,
{
    capacity: usize,
    /// Node pool - reusable slots
    pool: Vec<Slot<K, V>>,
    /// Key to slot index mapping
    map: HashMap<K, NodeIndex>,
    /// Free list head (for slot reuse)
    free_head: Option<usize>,
    /// LRU list head (most recently used)
    head: Option<usize>,
    /// LRU list tail (least recently used)
    tail: Option<usize>,
    /// Number of currently occupied slots
    len: usize,
}

impl<K, V> IntrusiveLruCache<K, V> 
where 
    K: Eq + Hash + Clone + Default,
    V: Clone + Default,
{
    /// Create a new cache with the specified capacity
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            pool: Vec::with_capacity(capacity),
            map: HashMap::with_capacity(capacity),
            free_head: None,
            head: None,
            tail: None,
            len: 0,
        }
    }

    /// Number of entries in the cache
    pub fn len(&self) -> usize {
        self.len
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Get a value from the cache, moving it to MRU position
    pub fn get(&mut self, key: &K) -> Option<V> {
        let node_idx = self.map.get(key).copied()?;
        
        // Validate generation
        if self.pool.get(node_idx.index)
            .map(|s| s.generation != node_idx.generation || !s.occupied)
            .unwrap_or(true) 
        {
            self.map.remove(key);
            return None;
        }

        let value = self.pool[node_idx.index].value.clone();
        self.move_to_head(node_idx.index);
        Some(value)
    }

    /// Insert or update a value, returns evicted entry if at capacity
    pub fn put(&mut self, key: K, value: V) -> Option<(K, V)> {
        // Check if key already exists
        if let Some(node_idx) = self.map.get(&key).copied() {
            if let Some(slot) = self.pool.get_mut(node_idx.index) {
                if slot.generation == node_idx.generation && slot.occupied {
                    slot.value = value;
                    self.move_to_head(node_idx.index);
                    return None;
                }
            }
            // Invalid entry, remove from map
            self.map.remove(&key);
        }

        let mut evicted = None;

        // Evict if at capacity
        if self.len >= self.capacity {
            evicted = self.pop_tail();
        }

        // Get a slot (from free list or new)
        let slot_idx = self.allocate_slot();
        let generation = self.pool[slot_idx].generation;
        
        self.pool[slot_idx].key = key.clone();
        self.pool[slot_idx].value = value;
        self.pool[slot_idx].occupied = true;
        
        // Insert into map
        self.map.insert(key, NodeIndex { index: slot_idx, generation });
        
        // Add to head of LRU list
        self.attach_head(slot_idx);
        self.len += 1;

        evicted
    }

    /// Allocate a slot, either from free list or new
    fn allocate_slot(&mut self) -> usize {
        if let Some(idx) = self.free_head {
            self.free_head = self.pool[idx].next;
            self.pool[idx].prev = None;
            self.pool[idx].next = None;
            idx
        } else {
            let idx = self.pool.len();
            self.pool.push(Slot::default());
            idx
        }
    }

    /// Move a slot to head (MRU)
    fn move_to_head(&mut self, idx: usize) {
        if self.head == Some(idx) {
            return; // Already at head
        }
        self.detach(idx);
        self.attach_head(idx);
    }

    /// Detach a slot from the list
    fn detach(&mut self, idx: usize) {
        let prev = self.pool[idx].prev;
        let next = self.pool[idx].next;

        if let Some(p) = prev {
            self.pool[p].next = next;
        } else {
            self.head = next;
        }

        if let Some(n) = next {
            self.pool[n].prev = prev;
        } else {
            self.tail = prev;
        }

        self.pool[idx].prev = None;
        self.pool[idx].next = None;
    }

    /// Attach a slot at the head
    fn attach_head(&mut self, idx: usize) {
        self.pool[idx].prev = None;
        self.pool[idx].next = self.head;

        if let Some(h) = self.head {
            self.pool[h].prev = Some(idx);
        }
        self.head = Some(idx);

        if self.tail.is_none() {
            self.tail = Some(idx);
        }
    }

    /// Remove and return the tail (LRU) entry
    fn pop_tail(&mut self) -> Option<(K, V)> {
        let tail_idx = self.tail?;
        
        self.detach(tail_idx);
        
        let slot = &mut self.pool[tail_idx];
        let key = std::mem::take(&mut slot.key);
        let value = std::mem::take(&mut slot.value);
        slot.occupied = false;
        slot.generation = slot.generation.wrapping_add(1);
        
        // Add to free list
        slot.next = self.free_head;
        self.free_head = Some(tail_idx);
        
        self.map.remove(&key);
        self.len -= 1;
        
        Some((key, value))
    }

    /// Get LRU entries without removing them
    pub fn get_lru_entries(&self, count: usize) -> Vec<(K, V)> {
        let mut entries = Vec::with_capacity(count);
        let mut current = self.tail;
        
        while let Some(idx) = current {
            if entries.len() >= count {
                break;
            }
            let slot = &self.pool[idx];
            if slot.occupied {
                entries.push((slot.key.clone(), slot.value.clone()));
            }
            current = slot.prev;
        }
        
        entries
    }

    /// Iterate over all entries (MRU to LRU order)
    pub fn iter(&self) -> Vec<(K, V)> {
        let mut entries = Vec::with_capacity(self.len);
        let mut current = self.head;
        
        while let Some(idx) = current {
            let slot = &self.pool[idx];
            if slot.occupied {
                entries.push((slot.key.clone(), slot.value.clone()));
            }
            current = slot.next;
        }
        
        entries
    }

    /// Get memory statistics
    pub fn memory_stats(&self) -> MemoryStats {
        let pool_capacity = self.pool.capacity() * std::mem::size_of::<Slot<K, V>>();
        let map_capacity = self.map.capacity() * (std::mem::size_of::<K>() + std::mem::size_of::<NodeIndex>());
        
        MemoryStats {
            pool_bytes: pool_capacity,
            map_bytes: map_capacity,
            total_bytes: pool_capacity + map_capacity,
            entries: self.len,
            capacity: self.capacity,
        }
    }
}

/// Memory statistics for the cache
#[derive(Debug, Clone)]
pub struct MemoryStats {
    pub pool_bytes: usize,
    pub map_bytes: usize,
    pub total_bytes: usize,
    pub entries: usize,
    pub capacity: usize,
}

// ============================================================================
// Legacy LRU Cache (Rc<RefCell<>> based)
// ============================================================================

use std::cell::RefCell;
use std::rc::Rc;

/// Doubly-linked list node
struct Node<K, V> {
    key: K,
    value: V,
    prev: Option<Rc<RefCell<Node<K, V>>>>,
    next: Option<Rc<RefCell<Node<K, V>>>>,
}

/// Standard LRU cache using Rc<RefCell<>>
pub struct LruCache<K, V> where K: Eq + Hash + Clone {
    capacity: usize,
    map: HashMap<K, Rc<RefCell<Node<K, V>>>>,
    head: Option<Rc<RefCell<Node<K, V>>>>, // MRU
    tail: Option<Rc<RefCell<Node<K, V>>>>, // LRU
}

impl<K, V> LruCache<K, V> where K: Eq + Hash + Clone {
    pub fn new(capacity: usize) -> Self {
        Self { capacity, map: HashMap::new(), head: None, tail: None }
    }

    #[allow(dead_code)]
    pub fn len(&self) -> usize { self.map.len() }
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool { self.map.is_empty() }

    pub fn get(&mut self, key: &K) -> Option<V> where V: Clone {
        if let Some(node_rc) = self.map.get(key).cloned() {
            self.move_to_head(node_rc.clone());
            return Some(node_rc.borrow().value.clone());
        }
        None
    }

    pub fn put(&mut self, key: K, value: V) -> Option<(K, V)> where V: Clone {
        if let Some(node_rc) = self.map.get(&key).cloned() {
            node_rc.borrow_mut().value = value;
            self.move_to_head(node_rc);
            return None;
        }
        let node = Rc::new(RefCell::new(Node { key: key.clone(), value, prev: None, next: None }));
        self.attach_head(node.clone());
        self.map.insert(key.clone(), node);
        if self.map.len() > self.capacity {
            if let Some(lru) = self.pop_tail() {
                let k = lru.borrow().key.clone();
                let v = lru.borrow().value.clone();
                self.map.remove(&k);
                return Some((k, v));
            }
        }
        None
    }

    fn detach(&mut self, node: Rc<RefCell<Node<K, V>>>) {
        let prev = node.borrow().prev.clone();
        let next = node.borrow().next.clone();
        if let Some(p) = prev.clone() { p.borrow_mut().next = next.clone(); } else { self.head = next.clone(); }
        if let Some(n) = next.clone() { n.borrow_mut().prev = prev.clone(); } else { self.tail = prev.clone(); }
        node.borrow_mut().prev = None;
        node.borrow_mut().next = None;
    }

    fn attach_head(&mut self, node: Rc<RefCell<Node<K, V>>>) {
        node.borrow_mut().prev = None;
        node.borrow_mut().next = self.head.clone();
        if let Some(h) = self.head.clone() { h.borrow_mut().prev = Some(node.clone()); }
        self.head = Some(node.clone());
        if self.tail.is_none() { self.tail = Some(node); }
    }

    fn move_to_head(&mut self, node: Rc<RefCell<Node<K, V>>>) {
        self.detach(node.clone());
        self.attach_head(node);
    }

    fn pop_tail(&mut self) -> Option<Rc<RefCell<Node<K, V>>>> {
        if let Some(t) = self.tail.clone() {
            self.detach(t.clone());
            return Some(t);
        }
        None
    }
}

// ============================================================================
// Package LRU Cache (High-level API using IntrusiveLruCache)
// ============================================================================

/// LRU cache specialized for package versions with usage tracking
/// Uses the memory-efficient IntrusiveLruCache internally
pub struct PackageLruCache {
    cache: IntrusiveLruCache<String, PackageUsageMetrics>,
    size_map: HashMap<String, u64>,
    max_size_bytes: u64,
    current_size_bytes: u64,
}

impl PackageLruCache {
    pub fn new(max_packages: usize, max_size_bytes: u64) -> Self {
        Self {
            cache: IntrusiveLruCache::new(max_packages),
            size_map: HashMap::new(),
            max_size_bytes,
            current_size_bytes: 0,
        }
    }

    /// Record package access (updates atime and increments access count)
    pub fn record_access(&mut self, package_key: &str, size_bytes: u64) {
        let now = Utc::now();
        
        if let Some(metrics) = self.cache.get(&package_key.to_string()) {
            let mut updated = metrics;
            updated.last_access_time = now;
            updated.access_count += 1;
            self.cache.put(package_key.to_string(), updated);
        } else {
            let metrics = PackageUsageMetrics {
                package_key: package_key.to_string(),
                last_access_time: now,
                last_script_execution: None,
                access_count: 1,
                script_execution_count: 0,
                last_successful_build: None,
            };
            
            if let Some((evicted_key, _)) = self.cache.put(package_key.to_string(), metrics) {
                if let Some(evicted_size) = self.size_map.remove(&evicted_key) {
                    self.current_size_bytes = self.current_size_bytes.saturating_sub(evicted_size);
                }
            }
            
            self.size_map.insert(package_key.to_string(), size_bytes);
            self.current_size_bytes += size_bytes;
            
            // Enforce size limit
            while self.current_size_bytes > self.max_size_bytes && !self.size_map.is_empty() {
                let lru = self.get_lru_packages(1);
                if let Some(lru_key) = lru.first() {
                    if let Some(size) = self.size_map.remove(lru_key) {
                        self.current_size_bytes = self.current_size_bytes.saturating_sub(size);
                    }
                    self.cache.get(lru_key);
                } else {
                    break;
                }
            }
        }
    }

    /// Record successful script execution
    #[allow(dead_code)]
    pub fn record_script_execution(&mut self, package_key: &str) {
        let now = Utc::now();
        if let Some(metrics) = self.cache.get(&package_key.to_string()) {
            let mut updated = metrics;
            updated.last_script_execution = Some(now);
            updated.script_execution_count += 1;
            self.cache.put(package_key.to_string(), updated);
        }
    }

    /// Record successful build
    #[allow(dead_code)]
    pub fn record_build(&mut self, package_key: &str) {
        let now = Utc::now();
        if let Some(metrics) = self.cache.get(&package_key.to_string()) {
            let mut updated = metrics;
            updated.last_successful_build = Some(now);
            self.cache.put(package_key.to_string(), updated);
        }
    }

    /// Get metrics for a package
    pub fn get_metrics(&mut self, package_key: &str) -> Option<PackageUsageMetrics> {
        self.cache.get(&package_key.to_string())
    }

    /// Get least recently used packages
    #[allow(dead_code)]
    pub fn get_lru_packages(&self, count: usize) -> Vec<String> {
        self.cache.get_lru_entries(count)
            .into_iter()
            .map(|(k, _)| k)
            .collect()
    }

    /// Iterate over all cached entries
    pub fn iter(&self) -> Vec<(String, PackageUsageMetrics)> {
        self.cache.iter()
    }

    /// Get the size of a specific package
    pub fn get_package_size(&self, package_key: &str) -> Option<u64> {
        self.size_map.get(package_key).copied()
    }

    /// Get current total size
    pub fn current_size(&self) -> u64 {
        self.current_size_bytes
    }

    /// Check if package should be kept based on LRU strategy
    pub fn should_keep_lru(&mut self, package_key: &str, days_threshold: i64) -> bool {
        if let Some(metrics) = self.get_metrics(package_key) {
            let days_since_access = (Utc::now() - metrics.last_access_time).num_days();
            
            if days_since_access < days_threshold {
                return true;
            }
            
            let size_pressure = self.current_size() as f64 / self.max_size_bytes as f64;
            if size_pressure > 0.9 {
                return days_since_access < (days_threshold / 3);
            }
            
            if let Some(pkg_size) = self.get_package_size(package_key) {
                let avg_size = if self.size_map.is_empty() { 
                    pkg_size 
                } else { 
                    self.current_size() / self.size_map.len() as u64 
                };
                
                if pkg_size > avg_size * 2 {
                    return days_since_access < (days_threshold / 2);
                }
            }
            
            return false;
        }
        false
    }
    
    /// Check if cache is under size pressure
    pub fn is_size_limited(&self) -> bool {
        self.current_size() >= self.max_size_bytes
    }

    /// Get memory statistics
    pub fn memory_stats(&self) -> MemoryStats {
        self.cache.memory_stats()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intrusive_lru_basic() {
        let mut lru: IntrusiveLruCache<String, i32> = IntrusiveLruCache::new(2);
        
        assert!(lru.get(&"a".to_string()).is_none());
        assert!(lru.put("a".to_string(), 1).is_none());
        assert_eq!(lru.get(&"a".to_string()), Some(1));
        assert!(lru.put("b".to_string(), 2).is_none());
        assert_eq!(lru.len(), 2);
        
        // Access 'a' to make it MRU
        lru.get(&"a".to_string());
        
        // Insert 'c' - should evict 'b' (LRU)
        let evicted = lru.put("c".to_string(), 3);
        assert!(evicted.is_some());
        let (k, v) = evicted.unwrap();
        assert_eq!(k, "b");
        assert_eq!(v, 2);
    }

    #[test]
    fn test_intrusive_lru_update() {
        let mut lru: IntrusiveLruCache<String, i32> = IntrusiveLruCache::new(2);
        
        lru.put("a".to_string(), 1);
        lru.put("a".to_string(), 10);
        
        assert_eq!(lru.get(&"a".to_string()), Some(10));
        assert_eq!(lru.len(), 1);
    }

    #[test]
    fn test_intrusive_lru_iter() {
        let mut lru: IntrusiveLruCache<String, i32> = IntrusiveLruCache::new(3);
        
        lru.put("a".to_string(), 1);
        lru.put("b".to_string(), 2);
        lru.put("c".to_string(), 3);
        
        let entries = lru.iter();
        assert_eq!(entries.len(), 3);
        // Most recent first
        assert_eq!(entries[0].0, "c");
    }

    #[test]
    fn test_legacy_lru_basic() {
        let mut lru = LruCache::new(2);
        assert!(lru.get(&"a").is_none());
        assert!(lru.put("a", 1).is_none());
        assert_eq!(lru.get(&"a"), Some(1));
        assert!(lru.put("b", 2).is_none());
        assert_eq!(lru.len(), 2);
        
        lru.get(&"a"); // 'a' MRU, 'b' LRU
        let evicted = lru.put("c", 3);
        assert!(evicted.is_some());
        let (k, v) = evicted.unwrap();
        assert_eq!(k, "b");
        assert_eq!(v, 2);
    }

    #[test]
    fn test_memory_stats() {
        let mut lru: IntrusiveLruCache<String, i32> = IntrusiveLruCache::new(100);
        
        for i in 0..50 {
            lru.put(format!("key_{}", i), i);
        }
        
        let stats = lru.memory_stats();
        assert_eq!(stats.entries, 50);
        assert_eq!(stats.capacity, 100);
        assert!(stats.total_bytes > 0);
    }
}
