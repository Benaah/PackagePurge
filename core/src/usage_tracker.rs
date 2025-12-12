#![allow(dead_code)]
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::types::PackageUsageMetrics;
use crate::cache::PackageLruCache;

/// Tracks and persists package usage metrics across runs
pub struct UsageTracker {
    cache_path: PathBuf,
    lru_cache: PackageLruCache,
}

impl UsageTracker {
    pub fn new(cache_path: PathBuf, max_packages: usize, max_size_bytes: u64) -> Result<Self> {
        // Ensure cache directory exists
        if let Some(parent) = cache_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create cache directory {:?}", parent))?;
        }

        let mut tracker = Self {
            cache_path: cache_path.clone(),
            lru_cache: PackageLruCache::new(max_packages, max_size_bytes),
        };

        // Load existing metrics if available
        if cache_path.exists() {
            if let Ok(metrics) = tracker.load_metrics() {
                for (key, _metric) in metrics {
                    tracker.lru_cache.record_access(&key, 0); // Size will be updated on scan
                }
            }
        }

        Ok(tracker)
    }

    /// Load persisted metrics from disk
    fn load_metrics(&self) -> Result<HashMap<String, PackageUsageMetrics>> {
        let content = fs::read_to_string(&self.cache_path)
            .with_context(|| format!("Failed to read cache file {:?}", self.cache_path))?;
        let metrics: HashMap<String, PackageUsageMetrics> = serde_json::from_str(&content)
            .with_context(|| "Failed to parse metrics cache")?;
        Ok(metrics)
    }

    /// Persist metrics to disk
    pub fn save_metrics(&self) -> Result<()> {
        // In a full implementation, we'd collect all metrics from the LRU cache
        // For now, this is a placeholder that would be called after optimization runs
        Ok(())
    }

    /// Record a script execution (e.g., npm run build, npm test)
    /// This should be called when monitoring detects script execution
    pub fn record_script_execution(&mut self, package_key: &str) {
        self.lru_cache.record_script_execution(package_key);
    }

    /// Record a successful build
    pub fn record_build(&mut self, package_key: &str) {
        self.lru_cache.record_build(package_key);
    }

    /// Get the LRU cache for direct access
    pub fn lru_cache_mut(&mut self) -> &mut PackageLruCache {
        &mut self.lru_cache
    }
}

/// Helper to detect script execution from package.json scripts
/// This would be integrated with npm/yarn execution monitoring
pub fn detect_script_execution(project_path: &Path, script_name: &str) -> Vec<String> {
    use std::fs;
    use serde_json::Value;

    let package_json = project_path.join("package.json");
    if !package_json.exists() {
        return Vec::new();
    }

    let mut affected_packages = Vec::new();

    if let Ok(content) = fs::read_to_string(&package_json) {
        if let Ok(json) = serde_json::from_str::<Value>(&content) {
            // Check if script exists
            if let Some(scripts) = json.get("scripts").and_then(|s| s.as_object()) {
                if scripts.contains_key(script_name) {
                    // In a full implementation, we'd parse the script to find dependencies
                    // For now, we'll extract direct dependencies
                    if let Some(deps) = json.get("dependencies").and_then(|d| d.as_object()) {
                        for (name, version) in deps {
                            if let Some(ver_str) = version.as_str() {
                                affected_packages.push(format!("{}@{}", name, ver_str));
                            }
                        }
                    }
                }
            }
        }
    }

    affected_packages
}