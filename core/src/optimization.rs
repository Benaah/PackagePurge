use anyhow::Result;
use chrono::{Duration, Utc};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::types::{DryRunReport, PlanItem, ScanOutput};

pub enum EvictionPolicy {
	MlThenArcThenLru,
	LruOnly,
}

pub struct RulesConfig {
	pub preserve_days: i64,
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
