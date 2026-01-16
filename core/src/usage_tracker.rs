//! Usage Tracker
//!
//! Tracks and persists package usage metrics across runs.
//! This data feeds into ML predictions for smarter eviction decisions.

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
    /// Default path for usage metrics cache
    pub fn default_cache_path() -> PathBuf {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        home.join(".packagepurge").join("usage_metrics.json")
    }

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
        // Collect all metrics from the LRU cache
        let mut metrics_map: HashMap<String, PackageUsageMetrics> = HashMap::new();
        
        // Iterate through the cache and collect metrics
        for (key, metrics) in self.lru_cache.iter() {
            metrics_map.insert(key, metrics);
        }
        
        // Persist to disk
        let content = serde_json::to_string_pretty(&metrics_map)
            .with_context(|| "Failed to serialize metrics")?;
        fs::write(&self.cache_path, content)
            .with_context(|| format!("Failed to write metrics to {:?}", self.cache_path))?;
        
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
    
    /// Get the LRU cache for read-only access
    pub fn lru_cache(&self) -> &PackageLruCache {
        &self.lru_cache
    }
}

/// Helper to detect script execution from package.json scripts
/// Parses the script command to find the actual tool being used
pub fn detect_script_execution(project_path: &Path, script_name: &str) -> Vec<String> {
    use serde_json::Value;

    let package_json = project_path.join("package.json");
    if !package_json.exists() {
        return Vec::new();
    }

    let mut affected_packages = Vec::new();

    if let Ok(content) = fs::read_to_string(&package_json) {
        if let Ok(json) = serde_json::from_str::<Value>(&content) {
            // Check if script exists and get its command
            if let Some(scripts) = json.get("scripts").and_then(|s| s.as_object()) {
                if let Some(script_cmd) = scripts.get(script_name).and_then(|v| v.as_str()) {
                    // Parse the script command to find the tool being used
                    // Examples: "vite build" -> vite, "jest --coverage" -> jest
                    let tools = extract_tools_from_script(script_cmd);
                    
                    // Look for these tools in dependencies
                    for dep_key in ["dependencies", "devDependencies"] {
                        if let Some(deps) = json.get(dep_key).and_then(|d| d.as_object()) {
                            for tool in &tools {
                                if let Some(version) = deps.get(tool).and_then(|v| v.as_str()) {
                                    affected_packages.push(format!("{}@{}", tool, version));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    affected_packages
}

/// Extract tool names from a script command
/// Handles common patterns like: "tool args", "npx tool", "node script.js", etc.
fn extract_tools_from_script(script: &str) -> Vec<String> {
    let mut tools = Vec::new();
    
    // Split by common command separators: &&, ||, ;, |
    let parts: Vec<&str> = script
        .split(|c| c == '&' || c == '|' || c == ';')
        .filter(|s| !s.is_empty())
        .collect();
    
    for part in parts {
        let words: Vec<&str> = part.trim().split_whitespace().collect();
        if words.is_empty() {
            continue;
        }
        
        let first = words[0];
        
        // Skip common shell/node commands and look for the actual tool
        match first {
            "npx" | "yarn" | "pnpm" | "npm" => {
                // The next word is likely the tool (e.g., "npx vite" -> vite)
                if words.len() > 1 {
                    let tool = words[1];
                    // Skip npm/yarn subcommands
                    if !["run", "exec", "dlx", "-c", "--"].contains(&tool) {
                        tools.push(tool.to_string());
                    } else if words.len() > 2 {
                        tools.push(words[2].to_string());
                    }
                }
            }
            "node" | "NODE_ENV=production" | "cross-env" => {
                // Skip to next meaningful word
                if words.len() > 1 {
                    let next = words[1];
                    if !next.starts_with('-') && !next.contains('=') && !next.ends_with(".js") {
                        tools.push(next.to_string());
                    }
                }
            }
            _ => {
                // First word is likely the tool itself
                if !first.starts_with('-') && !first.contains('=') {
                    tools.push(first.to_string());
                }
            }
        }
    }
    
    tools
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_tools_simple() {
        assert_eq!(extract_tools_from_script("vite build"), vec!["vite"]);
        assert_eq!(extract_tools_from_script("jest --coverage"), vec!["jest"]);
    }

    #[test]
    fn test_extract_tools_npx() {
        assert_eq!(extract_tools_from_script("npx vite build"), vec!["vite"]);
    }

    #[test]
    fn test_extract_tools_chained() {
        let tools = extract_tools_from_script("eslint . && prettier --check .");
        assert!(tools.contains(&"eslint".to_string()));
        assert!(tools.contains(&"prettier".to_string()));
    }
}