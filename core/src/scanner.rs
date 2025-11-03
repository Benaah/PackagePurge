use anyhow::Result;
use chrono::{DateTime, Utc};
use rayon::prelude::*;
use std::{fs, path::{Path, PathBuf}, time::SystemTime};
use walkdir::WalkDir;

use crate::types::{PackageRecord, ProjectRecord, ScanOutput, PackageManager};
use crate::lockfiles::{parse_npm_package_lock, parse_yarn_lock, parse_pnpm_lock};

fn to_utc(st: SystemTime) -> DateTime<Utc> { st.into() }

fn dir_size(path: &Path) -> u64 {
    let mut total: u64 = 0;
    for entry in WalkDir::new(path).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() {
            if let Ok(meta) = fs::metadata(entry.path()) {
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

fn collect_projects_and_edges(root: &Path) -> (Vec<ProjectRecord>, Vec<(String, String)>) {
    let mut projects = Vec::new();
    let mut edges: Vec<(String, String)> = Vec::new();
    for entry in WalkDir::new(root).max_depth(6).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() && entry.file_name() == "package.json" {
            let dir = entry.path().parent().unwrap_or(root);
            let manager = detect_manager_from_lock(dir);
            let mtime = fs::metadata(entry.path()).and_then(|m| m.modified()).ok()
                .map(to_utc).unwrap_or_else(|| Utc::now());
            // Basic dependency extraction from package.json
            let mut deps: Vec<(String, String)> = Vec::new();
            if let Ok(content) = fs::read_to_string(entry.path()) {
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
            // Lockfile DAG
            let lock_edges = match manager {
                Some(PackageManager::Npm) => parse_npm_package_lock(&dir.join("package-lock.json")),
                Some(PackageManager::Yarn) => parse_yarn_lock(&dir.join("yarn.lock")),
                Some(PackageManager::Pnpm) => parse_pnpm_lock(&dir.join("pnpm-lock.yaml")),
                None => Vec::new(),
            };
            edges.extend(lock_edges);

            projects.push(ProjectRecord {
                path: dir.to_string_lossy().to_string(),
                manager,
                dependencies: deps,
                mtime,
            });
        }
    }
    (projects, edges)
}

fn is_cache_dir(path: &Path) -> bool {
    let p = path.to_string_lossy().to_lowercase();
    p.ends_with(".npm") || p.contains("yarn/cache") || p.contains("pnpm/store")
}

pub fn scan(paths: &[PathBuf]) -> Result<ScanOutput> {
    let roots: Vec<PathBuf> = if paths.is_empty() { vec![std::env::current_dir()?] } else { paths.to_vec() };

    let mut all_projects: Vec<ProjectRecord> = Vec::new();
    let mut all_edges: Vec<(String, String)> = Vec::new();
    for root in &roots {
        let (projects, edges) = collect_projects_and_edges(root);
        all_projects.extend(projects);
        all_edges.extend(edges);
    }

    // Collect packages in node_modules and caches
    let mut package_dirs: Vec<PathBuf> = Vec::new();
    for root in &roots {
        for entry in WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
            if entry.file_type().is_dir() {
                let name = entry.file_name().to_string_lossy();
                if name == "node_modules" || is_cache_dir(entry.path()) {
                    package_dirs.push(entry.into_path());
                }
            }
        }
    }

    let packages: Vec<PackageRecord> = package_dirs.par_iter().flat_map(|dir| {
        WalkDir::new(dir).min_depth(1).max_depth(3).into_iter().filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_dir())
            .filter_map(|pkg_dir| {
                let pkg_path = pkg_dir.path().to_path_buf();
                let package_json = pkg_path.join("package.json");
                if !package_json.exists() { return None; }
                let meta = fs::metadata(&pkg_path).ok()?;
                let atime = meta.accessed().ok().map(to_utc).unwrap_or_else(|| Utc::now());
                let mtime = meta.modified().ok().map(to_utc).unwrap_or_else(|| Utc::now());
                let size = dir_size(&pkg_path);
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
            }).collect::<Vec<_>>()
    }).collect();

    Ok(ScanOutput { packages, projects: all_projects, edges: all_edges })
}
