use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::Hash;
use std::rc::Rc;
use chrono::Utc;
use crate::types::PackageUsageMetrics;

// Doubly-linked list node
struct Node<K, V> {
	key: K,
	value: V,
	prev: Option<Rc<RefCell<Node<K, V>>>>,
	next: Option<Rc<RefCell<Node<K, V>>>>,
}

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
			// Update value and move to head
			node_rc.borrow_mut().value = value;
			self.move_to_head(node_rc);
			return None;
		}
		// Insert new
		let node = Rc::new(RefCell::new(Node { key: key.clone(), value, prev: None, next: None }));
		self.attach_head(node.clone());
		self.map.insert(key.clone(), node);
		// Evict if over capacity
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

/// LRU cache specialized for package versions with usage tracking
pub struct PackageLruCache {
	cache: LruCache<String, PackageUsageMetrics>,
	size_map: HashMap<String, u64>,  // Track size per package
	max_size_bytes: u64,
	current_size_bytes: u64,
}

impl PackageLruCache {
	pub fn new(max_packages: usize, max_size_bytes: u64) -> Self {
		Self {
			cache: LruCache::new(max_packages),
			size_map: HashMap::new(),
			max_size_bytes,
			current_size_bytes: 0,
		}
	}

	/// Record package access (updates atime and increments access count)
	pub fn record_access(&mut self, package_key: &str, size_bytes: u64) {
		let now = Utc::now();
		if let Some(metrics) = self.cache.get(&package_key.to_string()) {
			// Update existing metrics
			let mut updated = metrics;
			updated.last_access_time = now;
			updated.access_count += 1;
			self.cache.put(package_key.to_string(), updated);
		} else {
			// Create new metrics
			let metrics = PackageUsageMetrics {
				package_key: package_key.to_string(),
				last_access_time: now,
				last_script_execution: None,
				access_count: 1,
				script_execution_count: 0,
				last_successful_build: None,
			};
			if let Some((evicted_key, _evicted_metrics)) = self.cache.put(package_key.to_string(), metrics) {
				// Decrement size when a package is evicted
				if let Some(evicted_size) = self.size_map.remove(&evicted_key) {
					self.current_size_bytes = self.current_size_bytes.saturating_sub(evicted_size);
				}
			}
			// Track size for this package
			self.size_map.insert(package_key.to_string(), size_bytes);
			self.current_size_bytes += size_bytes;
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

	/// Get metrics for a package (updates LRU position)
	pub fn get_metrics(&mut self, package_key: &str) -> Option<PackageUsageMetrics> {
		self.cache.get(&package_key.to_string())
	}

	/// Get least recently used packages (for eviction candidates)
	#[allow(dead_code)]
	/// Get least recently used packages (for eviction candidates)
	pub fn get_lru_packages(&self, count: usize) -> Vec<String> {
		// Iterate from tail (LRU) to head (MRU) and collect keys
		let mut packages = Vec::new();
		let mut current = self.cache.tail.clone();
		while let Some(node) = current {
			if packages.len() >= count {
				break;
			}
			packages.push(node.borrow().key.clone());
			current = node.borrow().prev.clone();
		}
		packages
	}

	/// Iterate over all cached entries (for persistence)
	pub fn iter(&self) -> Vec<(String, PackageUsageMetrics)> {
		let mut entries = Vec::new();
		let mut current = self.cache.head.clone();
		while let Some(node) = current {
			let borrowed = node.borrow();
			entries.push((borrowed.key.clone(), borrowed.value.clone()));
			current = borrowed.next.clone();
		}
		entries
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
			return days_since_access < days_threshold;
		}
		false
	}
}

#[cfg(test)]
mod tests {
	use super::LruCache;
	#[test]
	fn test_lru_basic() {
		let mut lru = LruCache::new(2);
		assert!(lru.get(&"a").is_none());
		assert!(lru.put("a", 1).is_none());
		assert_eq!(lru.get(&"a"), Some(1));
		assert!(lru.put("b", 2).is_none());
		assert_eq!(lru.len(), 2);
		// Insert c -> evict LRU (which should be 'a' after accessing 'a' it's MRU; LRU is 'b'?)
		// Access changes MRU; sequence ensures eviction is correct
		lru.get(&"a"); // 'a' MRU, 'b' LRU
		let evicted = lru.put("c", 3);
		assert!(evicted.is_some());
		let (k, v) = evicted.unwrap();
		assert_eq!(k, "b");
		assert_eq!(v, 2);
	}
}
