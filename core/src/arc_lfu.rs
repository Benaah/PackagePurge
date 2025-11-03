use std::collections::{HashMap, VecDeque};

// SLRU: probationary and protected segments, each LRU-like (front=MRU, back=LRU)
pub struct SlruPolicy {
	probationary: VecDeque<String>,
	protected: VecDeque<String>,
	cap_probationary: usize,
	cap_protected: usize,
	in_probationary: HashMap<String, bool>,
	in_protected: HashMap<String, bool>,
}

impl SlruPolicy {
	pub fn new(capacity: usize) -> Self {
		let cap_protected = (capacity as f32 * 0.8) as usize;
		let cap_probationary = capacity.saturating_sub(cap_protected);
		Self {
			probationary: VecDeque::new(),
			protected: VecDeque::new(),
			cap_probationary,
			cap_protected,
			in_probationary: HashMap::new(),
			in_protected: HashMap::new(),
		}
	}

	pub fn record_hit(&mut self, key: &str) {
		let k = key.to_string();
		if self.in_protected.remove(&k).is_some() {
			self.protected.retain(|x| x != &k);
			self.protected.push_front(k.clone());
			self.in_protected.insert(k, true);
			return;
		}
		if self.in_probationary.remove(&k).is_some() {
			// promote to protected
			self.probationary.retain(|x| x != &k);
			self.protected.push_front(k.clone());
			self.in_protected.insert(k.clone(), true);
			// enforce protected capacity
			while self.protected.len() > self.cap_protected {
				if let Some(v) = self.protected.pop_back() { self.in_protected.remove(&v); }
			}
			return;
		}
		// new entry goes to probationary
		self.probationary.push_front(k.clone());
		self.in_probationary.insert(k.clone(), true);
		while self.probationary.len() > self.cap_probationary {
			if let Some(v) = self.probationary.pop_back() { self.in_probationary.remove(&v); }
		}
	}

	pub fn select_victim(&mut self) -> Option<String> {
		if let Some(v) = self.probationary.pop_back() { self.in_probationary.remove(&v); return Some(v); }
		if let Some(v) = self.protected.pop_back() { self.in_protected.remove(&v); return Some(v); }
		None
	}
}

// Simple LFU: key->freq, and buckets freq->VecDeque keys. Evicts from lowest freq, oldest within bucket
pub struct SimpleLfu {
	freq: HashMap<String, usize>,
	buckets: HashMap<usize, VecDeque<String>>,
}

impl SimpleLfu {
	pub fn new() -> Self { Self { freq: HashMap::new(), buckets: HashMap::new() } }

	pub fn increment(&mut self, key: &str) {
		let k = key.to_string();
		let f = *self.freq.get(&k).unwrap_or(&0);
		if let Some(q) = self.buckets.get_mut(&f) { q.retain(|x| x != &k); }
		let nf = f + 1;
		self.freq.insert(k.clone(), nf);
		self.buckets.entry(nf).or_default().push_front(k);
	}

	pub fn victim(&mut self) -> Option<String> {
		if self.freq.is_empty() { return None; }
		let minf = *self.freq.values().min().unwrap_or(&0);
		if let Some(q) = self.buckets.get_mut(&minf) {
			if let Some(k) = q.pop_back() {
				self.freq.remove(&k);
				return Some(k);
			}
		}
		None
	}
}
