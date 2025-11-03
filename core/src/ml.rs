pub trait MlRecommender {
	fn is_safe_to_evict(&self, package_id: &str) -> Option<bool>;
}

pub struct NoopRecommender;
impl MlRecommender for NoopRecommender {
	fn is_safe_to_evict(&self, _package_id: &str) -> Option<bool> { None }
}
