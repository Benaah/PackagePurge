use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::Hash;
use std::rc::Rc;

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

	pub fn len(&self) -> usize { self.map.len() }
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
