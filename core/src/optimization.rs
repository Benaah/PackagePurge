use anyhow::Result;
use chrono::{Duration, Utc};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::types::{DryRunReport, PlanItem, ScanOutput, PackageUsageMetrics, ProjectMetadata, DeveloperBehavior};
use crate::symlink::SemanticDeduplication;
use crate::cache::PackageLruCache;
use crate::ml::{MlRecommender, PredictiveOptimizer};

#[allow(dead_code)]
pub enum EvictionPolicy {
	MlThenArcThenLru,
	LruOnly,
}

#[allow(dead_code)]
pub struct RulesConfig {
	pub preserve_days: i64,
	#[allow(dead_code)]
	pub enable_symlinking: bool,
	#[allow(dead_code)]
	pub enable_ml_prediction: bool,
	#[allow(dead_code)]
	pub lru_max_packages: usize,
	#[allow(dead_code)]
	pub lru_max_size_bytes: u64,
}

pub fn plan_basic_cleanup(scan: &ScanOutput, cfg: &RulesConfig) -> Result<DryRunReport> {
	let cutoff = Utc::now() - Duration::days(cfg.preserve_days);

	let mut used: HashSet<(String, String)> = HashSet::new();
	for proj in &scan.projects {
		for (n, v) in &proj.dependencies {
			used.insert((n.clone(), v.clone()));
		}
	}

	let mut seen_locations: HashMap<(String, String), Vec<PathBuf>> = HashMap::new();

	let mut items: Vec<PlanItem> = Vec::new();
	for pkg in &scan.packages {
		let key = (pkg.name.clone(), pkg.version.clone());
		seen_locations.entry(key.clone()).or_default().push(PathBuf::from(&pkg.path));

		let is_orphan = !used.contains(&key);
		let is_old = pkg.mtime < cutoff;

		if is_orphan || is_old {
			items.push(PlanItem {
				target_path: pkg.path.clone(),
				estimated_size_bytes: pkg.size_bytes,
				reason: if is_orphan { "orphaned".into() } else { "old".into() },
			});
		}
	}

	for (_key, paths) in seen_locations.into_iter() {
		if paths.len() > 1 {
			for p in paths.into_iter().skip(1) {
				items.push(PlanItem { target_path: p.to_string_lossy().to_string(), estimated_size_bytes: 0, reason: "duplicate".into() });
			}
		}
	}

	let total = items.iter().map(|i| i.estimated_size_bytes).sum();
	Ok(DryRunReport { items, total_estimated_bytes: total })
}

/// Optimization engine with symlinking and ML/LRU strategies
#[allow(dead_code)]
pub struct OptimizationEngine {
	deduplication: Option<SemanticDeduplication>,
	lru_cache: Option<PackageLruCache>,
	ml_predictor: Option<PredictiveOptimizer>,
	config: RulesConfig,
}

#[allow(dead_code)]
impl OptimizationEngine {
	pub fn new(config: RulesConfig) -> Result<Self> {
		let deduplication = if config.enable_symlinking {
			Some(SemanticDeduplication::new()?)
		} else {
			None
		};

		let lru_cache = Some(PackageLruCache::new(
			config.lru_max_packages,
			config.lru_max_size_bytes,
		));

		let ml_predictor = if config.enable_ml_prediction {
			Some(PredictiveOptimizer::new(config.preserve_days))
		} else {
			None
		};

		Ok(Self {
			deduplication,
			lru_cache,
			ml_predictor,
			config,
		})
	}

	/// Plan cleanup with symlinking and ML/LRU optimization
	pub fn plan_optimized_cleanup(
		&mut self,
		scan: &ScanOutput,
	) -> Result<DryRunReport> {
		let cutoff = Utc::now() - Duration::days(self.config.preserve_days);

		// Build usage metrics map from scan
		let mut usage_map: HashMap<String, PackageUsageMetrics> = HashMap::new();
		for pkg in &scan.packages {
			let key = format!("{}@{}", pkg.name, pkg.version);
			let metrics = PackageUsageMetrics {
				package_key: key.clone(),
				last_access_time: pkg.atime,
				last_script_execution: None, // Would be populated from execution tracking
				access_count: 1, // Would be tracked over time
				script_execution_count: 0,
				last_successful_build: None,
			};
			usage_map.insert(key, metrics);
		}

		// Build project metadata map
		let mut project_map: HashMap<String, ProjectMetadata> = HashMap::new();
		for proj in &scan.projects {
			let metadata = ProjectMetadata {
				path: proj.path.clone(),
				project_type: detect_project_type(&proj.path),
				last_commit_date: None, // Would be populated from git
				dependency_count: proj.dependencies.len(),
				last_modified: proj.mtime,
			};
			project_map.insert(proj.path.clone(), metadata);
		}

		let mut used: HashSet<(String, String)> = HashSet::new();
		for proj in &scan.projects {
			for (n, v) in &proj.dependencies {
				used.insert((n.clone(), v.clone()));
			}
		}

		let mut seen_locations: HashMap<(String, String), Vec<PathBuf>> = HashMap::new();
		let mut items: Vec<PlanItem> = Vec::new();
		let mut symlink_candidates: Vec<(PathBuf, String, String)> = Vec::new();

		for pkg in &scan.packages {
			let key = (pkg.name.clone(), pkg.version.clone());
			seen_locations.entry(key.clone()).or_default().push(PathBuf::from(&pkg.path));

			let package_key = format!("{}@{}", pkg.name, pkg.version);
			let is_orphan = !used.contains(&key);
			let is_old = pkg.mtime < cutoff;

			// Record access in LRU cache
			if let Some(ref mut cache) = self.lru_cache {
				cache.record_access(&package_key, pkg.size_bytes);
			}

			// Check ML prediction
			let should_keep_ml = if let Some(ref predictor) = self.ml_predictor {
				if let (Some(metrics), Some(proj_path)) = (usage_map.get(&package_key), pkg.project_paths.first()) {
					if let Some(project_meta) = project_map.get(proj_path) {
						let behavior = DeveloperBehavior {
							npm_commands_executed: Vec::new(), // Would be populated from tracking
							file_access_frequency: 0,
							days_since_last_build: None,
						};
						predictor.should_keep(&package_key, metrics, project_meta, &behavior)
					} else {
						true // Conservative: keep if no project metadata
					}
				} else {
					true
				}
			} else {
				true
			};

			// Check LRU strategy
			let should_keep_lru = if let Some(ref mut cache) = self.lru_cache {
				cache.should_keep_lru(&package_key, self.config.preserve_days)
			} else {
				true
			};

			// Check if cache is under size pressure
			let cache_size_limited = if let Some(ref cache) = self.lru_cache {
				cache.is_size_limited()
			} else {
				false
			};

			// Determine if package should be removed
			if is_orphan || (is_old && !should_keep_ml && !should_keep_lru) {
				items.push(PlanItem {
					target_path: pkg.path.clone(),
					estimated_size_bytes: pkg.size_bytes,
					reason: if is_orphan {
						"orphaned".into()
					} else if !should_keep_ml {
						"ml_predicted_unused".into()
					} else if cache_size_limited {
						"size_pressure".into()
					} else {
						"old".into()
					},
				});
			}

			// Collect symlink candidates (duplicates)
			if let Some(ref _dedup) = self.deduplication {
				if seen_locations.get(&key).map(|v| v.len()).unwrap_or(0) > 1 {
					symlink_candidates.push((PathBuf::from(&pkg.path), pkg.name.clone(), pkg.version.clone()));
				}
			}
		}

		// Process symlink candidates (in dry run, just mark them)
		for (path, _name, _version) in symlink_candidates {
			items.push(PlanItem {
				target_path: path.to_string_lossy().to_string(),
				estimated_size_bytes: 0,
				reason: "duplicate_symlink_candidate".into(),
			});
		}

		let total = items.iter().map(|i| i.estimated_size_bytes).sum();
		Ok(DryRunReport { items, total_estimated_bytes: total })
	}

	/// Execute symlinking for duplicate packages
	pub fn execute_symlinking(&self, scan: &ScanOutput) -> Result<usize> {
		if let Some(ref dedup) = self.deduplication {
			let mut seen: HashMap<(String, String), PathBuf> = HashMap::new();
			let mut symlinked_count = 0;

			for pkg in &scan.packages {
				let key = (pkg.name.clone(), pkg.version.clone());
				
				// Keep first occurrence as canonical
				let canonical = seen.entry(key.clone()).or_insert_with(|| PathBuf::from(&pkg.path));
				
				// Symlink duplicates
				if canonical.to_string_lossy() != pkg.path {
					let pkg_path = PathBuf::from(&pkg.path);
					if let Err(e) = dedup.deduplicate_package(&pkg_path, &pkg.name, &pkg.version) {
						eprintln!("Failed to symlink {:?}: {}", pkg_path, e);
					} else {
						symlinked_count += 1;
					}
				}
			}

			Ok(symlinked_count)
		} else {
			Ok(0)
		}
	}
}

fn detect_project_type(project_path: &str) -> String {
	use std::fs;
	use std::path::Path;
	
	let path = Path::new(project_path);
	let package_json = path.join("package.json");
	
	// Check package.json for project type indicators
	if package_json.exists() {
		if let Ok(content) = fs::read_to_string(&package_json) {
			if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
				// Check for framework-specific dependencies
				if let Some(deps) = json.get("dependencies").and_then(|d| d.as_object()) {
					if deps.contains_key("react") || deps.contains_key("next") {
						return "react".into();
					}
					if deps.contains_key("vue") || deps.contains_key("nuxt") {
						return "vue".into();
					}
					if deps.contains_key("angular") || deps.contains_key("@angular/core") {
						return "angular".into();
					}
				}
				
				// Check devDependencies
				if let Some(dev_deps) = json.get("devDependencies").and_then(|d| d.as_object()) {
					if dev_deps.contains_key("typescript") || dev_deps.contains_key("tsc") {
						return "typescript".into();
					}
				}
			}
		}
		
		// Check for TypeScript config files
		if path.join("tsconfig.json").exists() {
			return "typescript".into();
		}
		
		// Check for Next.js
		if path.join("next.config.js").exists() || path.join("next.config.ts").exists() {
			return "nextjs".into();
		}
		
		// Check path-based heuristics as fallback
		let path_lower = project_path.to_lowercase();
		if path_lower.contains("react") || path_lower.contains("next") {
			return "react".into();
		}
		if path_lower.contains("typescript") || path_lower.contains("ts") {
			return "typescript".into();
		}
	}
	
	"node".into()
}
