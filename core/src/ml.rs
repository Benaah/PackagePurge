use chrono::Utc;
use crate::types::{PackageUsageMetrics, ProjectMetadata, DeveloperBehavior};

pub trait MlRecommender {
	fn is_safe_to_evict(&self, package_id: &str) -> Option<bool>;
	fn should_keep(&self, package_id: &str, metrics: &PackageUsageMetrics, project: &ProjectMetadata, behavior: &DeveloperBehavior) -> bool;
}

pub struct NoopRecommender;
impl MlRecommender for NoopRecommender {
	fn is_safe_to_evict(&self, _package_id: &str) -> Option<bool> { None }
	fn should_keep(&self, _package_id: &str, _metrics: &PackageUsageMetrics, _project: &ProjectMetadata, _behavior: &DeveloperBehavior) -> bool {
		true // Conservative: keep by default
	}
}

/// Predictive Optimizer using rule-based ML (can be extended with actual ML models)
#[allow(dead_code)]
pub struct PredictiveOptimizer {
	/// Keep threshold in days (packages used within this period are likely needed)
	prediction_window_days: i64,
}

impl PredictiveOptimizer {
	pub fn new(prediction_window_days: i64) -> Self {
		Self { prediction_window_days }
	}

	/// Extract features from package metadata for ML prediction
	fn extract_features(
		&self,
		metrics: &PackageUsageMetrics,
		project: &ProjectMetadata,
		behavior: &DeveloperBehavior,
	) -> Vec<f64> {
		let now = Utc::now();
		
		// Feature 1: Days since last access
		let days_since_access = (now - metrics.last_access_time).num_days() as f64;
		
		// Feature 2: Days since last script execution
		let days_since_script = metrics.last_script_execution
			.map(|t| (now - t).num_days() as f64)
			.unwrap_or(365.0); // High value if never executed
		
		// Feature 3: Days since last successful build
		let days_since_build = metrics.last_successful_build
			.map(|t| (now - t).num_days() as f64)
			.unwrap_or(365.0);
		
		// Feature 4: Access frequency (normalized)
		let access_frequency = metrics.access_count as f64 / 100.0; // Normalize
		
		// Feature 5: Script execution frequency
		let script_frequency = metrics.script_execution_count as f64 / 10.0;
		
		// Feature 6: Project activity (days since last commit)
		let days_since_commit = project.last_commit_date
			.map(|t| (now - t).num_days() as f64)
			.unwrap_or(365.0);
		
		// Feature 7: Project type score (higher for active project types)
		let project_type_score = match project.project_type.as_str() {
			"react" | "typescript" | "nextjs" => 1.0,
			"node" => 0.8,
			_ => 0.5,
		};
		
		// Feature 8: Dependency count (more deps = more likely to need packages)
		let dep_score = (project.dependency_count as f64 / 100.0).min(1.0);
		
		// Feature 9: Days since last build (from behavior)
		let behavior_days_since_build = behavior.days_since_last_build
			.map(|d| d as f64)
			.unwrap_or(365.0);
		
		// Feature 10: File access frequency
		let file_access_score = (behavior.file_access_frequency as f64 / 1000.0).min(1.0);
		
		vec![
			days_since_access,
			days_since_script,
			days_since_build,
			access_frequency,
			script_frequency,
			days_since_commit,
			project_type_score,
			dep_score,
			behavior_days_since_build,
			file_access_score,
		]
	}

	/// Predict whether package should be kept (binary classification)
	/// Returns true if package is likely needed in the next prediction_window_days
	pub fn predict_keep(
		&self,
		metrics: &PackageUsageMetrics,
		project: &ProjectMetadata,
		behavior: &DeveloperBehavior,
	) -> bool {
		let features = self.extract_features(metrics, project, behavior);
		
		// Simple rule-based classifier (can be replaced with actual ML model)
		// This implements a heuristic that mimics what a trained model would do
		
		// Rule 1: Recently accessed packages are likely needed
		let days_since_access = features[0];
		if days_since_access < 7.0 {
			return true; // Keep if accessed in last week
		}
		
		// Rule 2: Recently used in scripts
		let days_since_script = features[1];
		if days_since_script < 14.0 {
			return true; // Keep if used in script in last 2 weeks
		}
		
		// Rule 3: Recently built successfully
		let days_since_build = features[2];
		if days_since_build < 30.0 {
			return true; // Keep if built in last month
		}
		
		// Rule 4: High access frequency
		let access_frequency = features[3];
		if access_frequency > 0.5 {
			return true; // Keep if frequently accessed
		}
		
		// Rule 5: Active project with recent commits
		let days_since_commit = features[5];
		let project_type_score = features[6];
		if days_since_commit < 30.0 && project_type_score > 0.7 {
			return true; // Keep if project is active
		}
		
		// Rule 6: Weighted score combining all features
		// This is a simplified logistic regression-like decision
		let score = self.compute_keep_score(&features);
		score > 0.5
	}

	/// Compute a keep score (0.0 to 1.0) based on features
	/// This mimics a logistic regression output
	fn compute_keep_score(&self, features: &[f64]) -> f64 {
		// Weighted combination of features (weights learned from training data in real ML)
		// For now, use heuristic weights
		let weights = vec![
			-0.1,  // days_since_access (negative: more days = lower score)
			-0.05, // days_since_script
			-0.03, // days_since_build
			0.3,   // access_frequency (positive: more access = higher score)
			0.2,   // script_frequency
			-0.02, // days_since_commit
			0.15,  // project_type_score
			0.1,   // dep_score
			-0.03, // behavior_days_since_build
			0.1,   // file_access_score
		];
		
		let mut score = 0.5; // Base score
		for (feature, weight) in features.iter().zip(weights.iter()) {
			score += feature * weight;
		}
		
		// Apply sigmoid-like function to bound between 0 and 1
		1.0 / (1.0 + (-score).exp())
	}
}

impl MlRecommender for PredictiveOptimizer {
	fn is_safe_to_evict(&self, _package_id: &str) -> Option<bool> {
		None // Use should_keep instead
	}

	fn should_keep(
		&self,
		_package_id: &str,
		metrics: &PackageUsageMetrics,
		project: &ProjectMetadata,
		behavior: &DeveloperBehavior,
	) -> bool {
		self.predict_keep(metrics, project, behavior)
	}
}
