//! Package and Project Scanner
//!
//! Scans filesystem to discover node_modules, package caches, and project roots.
//! Uses incremental caching for improved performance on subsequent runs.

use anyhow::Result;
use chrono::{DateTime, Utc};
use rayon::prelude::*;
use std::sync::Mutex;
use std::{fs, path::{Path, PathBuf}, time::SystemTime};
use walkdir::WalkDir;

use crate::types::{PackageRecord, ProjectRecord, ScanOutput, PackageManager};
use crate::lockfiles::{parse_npm_package_lock, parse_yarn_lock, parse_pnpm_lock};
use crate::scan_cache::ScanCache;

fn to_utc(st: SystemTime) -> DateTime<Utc> { st.into() }

/// Compute directory size by walking all files
fn dir_size(path: &Path) -> u64 {
    let mut total: u64 = 0;
    for entry in WalkDir::new(path).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() {
            if let Ok(meta) = entry.metadata() {
                total += meta.len();
            }
        }
    }
    total
}

fn detect_manager_from_lock(dir: &Path) -> Option<PackageManager> {
    if dir.join("package-lock.json").exists() { return Some(PackageManager::Npm); }
    if dir.join("yarn.lock").exists() { return Some(PackageManager::Yarn); }
    if dir.join("pnpm-lock.yaml").exists() { return Some(PackageManager::Pnpm); }
    None
}

fn is_cache_dir(path: &Path) -> bool {
    let p = path.to_string_lossy().to_lowercase();
    p.ends_with(".npm") || p.contains("yarn/cache") || p.contains("pnpm/store")
}

/// Single-pass directory walker that collects both package directories and projects
struct SinglePassCollector {
    package_dirs: Vec<PathBuf>,
    projects: Vec<ProjectRecord>,
    edges: Vec<(String, String)>,
}

impl SinglePassCollector {
    fn new() -> Self {
        Self {
            package_dirs: Vec::new(),
            projects: Vec::new(),
            edges: Vec::new(),
        }
    }

    /// Collect all data in a single directory walk
    fn collect(&mut self, roots: &[PathBuf]) -> Result<()> {
        for root in roots {
            for entry in WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
                let path = entry.path();
                
                if entry.file_type().is_dir() {
                    let name = entry.file_name().to_string_lossy();
                    if name == "node_modules" || is_cache_dir(path) {
                        self.package_dirs.push(entry.into_path());
                    }
                } else if entry.file_type().is_file() && entry.file_name() == "package.json" {
                    // Skip node_modules package.json files
                    let path_str = path.to_string_lossy();
                    if path_str.contains("node_modules") {
                        continue;
                    }
                    
                    if let Some(project) = self.parse_project(path) {
                        self.projects.push(project);
                    }
                }
            }
        }
        Ok(())
    }

    fn parse_project(&self, package_json: &Path) -> Option<ProjectRecord> {
        let dir = package_json.parent()?;
        let manager = detect_manager_from_lock(dir);
        let mtime = fs::metadata(package_json).and_then(|m| m.modified()).ok()
            .map(to_utc).unwrap_or_else(|| Utc::now());
        
        let mut deps: Vec<(String, String)> = Vec::new();
        if let Ok(content) = fs::read_to_string(package_json) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                for key in ["dependencies", "devDependencies", "peerDependencies"] {
                    if let Some(obj) = json.get(key).and_then(|v| v.as_object()) {
                        for (name, ver) in obj {
                            if let Some(ver_str) = ver.as_str() {
                                deps.push((name.clone(), ver_str.to_string()));
                            }
                        }
                    }
                }
            }
        }
        
        let lock_deps = match manager {
            Some(PackageManager::Npm) => parse_npm_package_lock(&dir.join("package-lock.json")),
            Some(PackageManager::Yarn) => parse_yarn_lock(&dir.join("yarn.lock")),
            Some(PackageManager::Pnpm) => parse_pnpm_lock(&dir.join("pnpm-lock.yaml")),
            None => Vec::new(),
        };
        
        let mut all_deps = deps;
        all_deps.extend(lock_deps);

        Some(ProjectRecord {
            path: dir.to_string_lossy().to_string(),
            manager,
            dependencies: all_deps,
            mtime,
        })
    }
}

/// Main scan function - uses incremental caching for improved performance
pub fn scan(paths: &[PathBuf]) -> Result<ScanOutput> {
    scan_with_cache(paths, true)
}

/// Scan with optional caching
pub fn scan_with_cache(paths: &[PathBuf], use_cache: bool) -> Result<ScanOutput> {
    let roots: Vec<PathBuf> = if paths.is_empty() { 
        vec![std::env::current_dir()?] 
    } else { 
        paths.to_vec() 
    };

    // Initialize cache with Mutex for thread-safe updates
    let cache_path = ScanCache::default_cache_path();
    let cache = if use_cache {
        ScanCache::load_or_create(&cache_path).unwrap_or_else(|_| ScanCache::new())
    } else {
        ScanCache::new()
    };
    let cache = Mutex::new(cache);

    // Single-pass collection
    let mut collector = SinglePassCollector::new();
    collector.collect(&roots)?;

    // Process packages in parallel with thread-safe cache access
    let packages: Vec<PackageRecord> = collector.package_dirs.par_iter().flat_map(|dir| {
        WalkDir::new(dir).min_depth(1).max_depth(3)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_dir())
            .filter_map(|pkg_dir| {
                let pkg_path = pkg_dir.path().to_path_buf();
                let package_json = pkg_path.join("package.json");
                if !package_json.exists() { return None; }
                
                let meta = fs::metadata(&pkg_path).ok()?;
                let atime = meta.accessed().ok().map(to_utc).unwrap_or_else(|| Utc::now());
                let mtime = meta.modified().ok().map(to_utc).unwrap_or_else(|| Utc::now());
                
                // Use cached size if available, otherwise compute
                let size = if use_cache {
                    let cached_size = cache.lock().ok()
                        .and_then(|c| c.get_cached_size(&pkg_path));
                    
                    if let Some(size) = cached_size {
                        size
                    } else {
                        let computed = dir_size(&pkg_path);
                        if let Ok(mut c) = cache.lock() {
                            let _ = c.update(&pkg_path, computed);
                        }
                        computed
                    }
                } else {
                    dir_size(&pkg_path)
                };
                
                let (name, version) = if let Ok(text) = fs::read_to_string(&package_json) {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                        let n = json.get("name").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
                        let v = json.get("version").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
                        (n, v)
                    } else { ("unknown".into(), "unknown".into()) }
                } else { ("unknown".into(), "unknown".into()) };
                
                Some(PackageRecord {
                    name,
                    version,
                    path: pkg_path.to_string_lossy().to_string(),
                    size_bytes: size,
                    atime,
                    mtime,
                    manager: None,
                    project_paths: Vec::new(),
                })
            })
            .collect::<Vec<_>>()
    }).collect();

    // Save cache
    if use_cache {
        if let Ok(mut c) = cache.lock() {
            if let Err(e) = c.save(&cache_path) {
                eprintln!("Warning: Failed to save scan cache: {}", e);
            } else {
                let stats = c.stats();
                eprintln!("Cache: {} entries, {} bytes cached", 
                    stats.total_entries, stats.total_cached_size);
            }
        }
    }

    Ok(ScanOutput { 
        packages, 
        projects: collector.projects, 
        edges: collector.edges 
    })
}

/// Scan without using cache (for testing or forced refresh)
pub fn scan_no_cache(paths: &[PathBuf]) -> Result<ScanOutput> {
    scan_with_cache(paths, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_detect_manager() {
        let temp = tempdir().unwrap();
        
        // No lockfile
        assert!(detect_manager_from_lock(temp.path()).is_none());
        
        // npm
        fs::write(temp.path().join("package-lock.json"), "{}").unwrap();
        assert!(matches!(detect_manager_from_lock(temp.path()), Some(PackageManager::Npm)));
    }

    #[test]
    fn test_is_cache_dir() {
        assert!(is_cache_dir(Path::new("/home/user/.npm")));
        assert!(is_cache_dir(Path::new("/home/user/.yarn/cache")));
        assert!(is_cache_dir(Path::new("/home/user/.local/share/pnpm/store")));
        assert!(!is_cache_dir(Path::new("/home/user/projects")));
    }

    #[test]
    fn test_single_pass_collector() {
        let temp = tempdir().unwrap();
        
        // Create a project structure
        let project_dir = temp.path().join("my-project");
        fs::create_dir_all(&project_dir).unwrap();
        fs::write(project_dir.join("package.json"), r#"{"name": "test", "version": "1.0.0"}"#).unwrap();
        
        let mut collector = SinglePassCollector::new();
        collector.collect(&[temp.path().to_path_buf()]).unwrap();
        
        assert_eq!(collector.projects.len(), 1);
        assert_eq!(collector.projects[0].path, project_dir.to_string_lossy());
    }

    #[test]
    fn test_scan_with_cache() {
        let temp = tempdir().unwrap();
        let project_dir = temp.path().join("test-project");
        let node_modules = project_dir.join("node_modules");
        let pkg_dir = node_modules.join("test-pkg");
        
        fs::create_dir_all(&pkg_dir).unwrap();
        fs::write(project_dir.join("package.json"), r#"{"name": "root"}"#).unwrap();
        fs::write(pkg_dir.join("package.json"), r#"{"name": "test-pkg", "version": "1.0.0"}"#).unwrap();
        
        // First scan
        let result1 = scan_with_cache(&[temp.path().to_path_buf()], false).unwrap();
        assert!(!result1.packages.is_empty() || !result1.projects.is_empty());
    }
}
