//! Safety Module - Quarantine and Rollback
//!
//! Provides safe package cleanup with:
//! - Quarantine system (move-then-verify pattern)
//! - Lazy SHA256 (computed only when needed)
//! - Size quotas and automatic cleanup
//! - Rollback capability

use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fs, path::{Path, PathBuf}};

use crate::types::QuarantineRecord;

/// Quarantine manager configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuarantineConfig {
    /// Maximum quarantine size in GB (0 = unlimited)
    pub max_size_gb: u64,
    /// Retention period in days (0 = keep forever)
    pub retention_days: i64,
    /// Maximum number of entries to keep (0 = unlimited)
    pub max_entries: usize,
}

impl Default for QuarantineConfig {
    fn default() -> Self {
        Self {
            max_size_gb: 10,       // 10GB default
            retention_days: 30,    // 30 days default
            max_entries: 200,      // 200 entries default
        }
    }
}

/// Quarantine statistics
#[derive(Debug, Clone, Serialize)]
pub struct QuarantineStats {
    pub total_entries: usize,
    pub total_size_bytes: u64,
    pub oldest_entry_days: i64,
    pub entries_over_retention: usize,
}

fn quarantine_dir() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".packagepurge").join("quarantine")
}

fn index_path() -> PathBuf {
    quarantine_dir().join("index.json")
}

fn config_path() -> PathBuf {
    quarantine_dir().join("config.json")
}

fn read_index() -> Vec<QuarantineRecord> {
    let p = index_path();
    if let Ok(text) = fs::read_to_string(&p) {
        if let Ok(list) = serde_json::from_str::<Vec<QuarantineRecord>>(&text) { 
            return list; 
        }
    }
    Vec::new()
}

fn write_index(list: &[QuarantineRecord]) -> Result<()> {
    let data = serde_json::to_string_pretty(list)?;
    fs::write(index_path(), data).context("Failed to write quarantine index")?;
    Ok(())
}

/// Load quarantine configuration
pub fn load_config() -> QuarantineConfig {
    let p = config_path();
    if let Ok(text) = fs::read_to_string(&p) {
        if let Ok(config) = serde_json::from_str::<QuarantineConfig>(&text) {
            return config;
        }
    }
    QuarantineConfig::default()
}

/// Save quarantine configuration
pub fn save_config(config: &QuarantineConfig) -> Result<()> {
    let qdir = quarantine_dir();
    fs::create_dir_all(&qdir).ok();
    let data = serde_json::to_string_pretty(config)?;
    fs::write(config_path(), data).context("Failed to save quarantine config")?;
    Ok(())
}

/// Lazy SHA256 computation - returns a closure that computes on demand
#[allow(dead_code)]
pub fn sha256_dir_lazy(path: PathBuf) -> impl FnOnce() -> Result<(String, u64)> {
    move || sha256_dir(&path)
}

/// Compute SHA256 of directory contents
fn sha256_dir(path: &Path) -> Result<(String, u64)> {
    let mut hasher = Sha256::new();
    let mut total: u64 = 0;
    
    for entry in walkdir::WalkDir::new(path).into_iter().filter_map(|e| e.ok()) {
        let p = entry.path();
        hasher.update(p.to_string_lossy().as_bytes());
        if entry.file_type().is_file() {
            let data = fs::read(p)?;
            total += data.len() as u64;
            hasher.update(&data);
        }
    }
    Ok((hex::encode(hasher.finalize()), total))
}

/// Quick size estimate without full hash (faster for quota checks)
fn quick_size(path: &Path) -> u64 {
    walkdir::WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum()
}

/// Get quarantine statistics
pub fn get_quarantine_stats() -> QuarantineStats {
    let list = read_index();
    let now = Utc::now();
    let config = load_config();
    
    let total_size: u64 = list.iter().map(|r| r.size_bytes).sum();
    let oldest_days = list.iter()
        .map(|r| (now - r.created_at).num_days())
        .max()
        .unwrap_or(0);
    
    let over_retention = if config.retention_days > 0 {
        list.iter()
            .filter(|r| (now - r.created_at).num_days() > config.retention_days)
            .count()
    } else {
        0
    };
    
    QuarantineStats {
        total_entries: list.len(),
        total_size_bytes: total_size,
        oldest_entry_days: oldest_days,
        entries_over_retention: over_retention,
    }
}

/// Cleanup old entries based on configuration
/// Returns number of entries cleaned and bytes freed
pub fn cleanup_quarantine() -> Result<(usize, u64)> {
    let config = load_config();
    let mut list = read_index();
    let now = Utc::now();
    
    let mut cleaned_count = 0;
    let mut bytes_freed: u64 = 0;
    
    // Sort by age (oldest first) for processing
    list.sort_by_key(|r| r.created_at);
    
    let mut to_remove = Vec::new();
    
    // Check retention period
    if config.retention_days > 0 {
        for rec in &list {
            if (now - rec.created_at).num_days() > config.retention_days {
                to_remove.push(rec.id.clone());
            }
        }
    }
    
    // Check max entries
    if config.max_entries > 0 && list.len() > config.max_entries {
        let excess = list.len() - config.max_entries;
        for rec in list.iter().take(excess) {
            if !to_remove.contains(&rec.id) {
                to_remove.push(rec.id.clone());
            }
        }
    }
    
    // Check size quota
    if config.max_size_gb > 0 {
        let max_bytes = config.max_size_gb * 1024 * 1024 * 1024;
        let total: u64 = list.iter().map(|r| r.size_bytes).sum();
        
        if total > max_bytes {
            let mut current_size = total;
            for rec in &list {
                if current_size <= max_bytes {
                    break;
                }
                if !to_remove.contains(&rec.id) {
                    to_remove.push(rec.id.clone());
                    current_size -= rec.size_bytes;
                }
            }
        }
    }
    
    // Delete files and update index
    for id in &to_remove {
        if let Some(rec) = list.iter().find(|r| &r.id == id) {
            let qpath = PathBuf::from(&rec.quarantine_path);
            if qpath.exists() {
                if let Ok(()) = fs::remove_dir_all(&qpath) {
                    bytes_freed += rec.size_bytes;
                    cleaned_count += 1;
                }
            }
        }
    }
    
    // Remove from index
    list.retain(|r| !to_remove.contains(&r.id));
    write_index(&list)?;
    
    Ok((cleaned_count, bytes_freed))
}

/// Move target to quarantine with lazy SHA256
/// SHA256 is only computed after move succeeds (optimizes for common case)
pub fn move_to_quarantine(target: &Path) -> Result<QuarantineRecord> {
    // Run cleanup first if needed
    let stats = get_quarantine_stats();
    let config = load_config();
    
    if config.max_entries > 0 && stats.total_entries >= config.max_entries {
        cleanup_quarantine()?;
    }
    
    let qdir = quarantine_dir();
    fs::create_dir_all(&qdir).ok();
    
    let id = format!("{}", Utc::now().timestamp_nanos_opt().unwrap_or(0));
    let qpath = qdir.join(format!("{}_{}", id, target.file_name().unwrap_or_default().to_string_lossy()));
    
    // Get size first (faster than full hash)
    let size = quick_size(target);
    
    // Perform the move
    if let Err(e) = fs::rename(target, &qpath) {
        // Handle cross-device link errors - try copy-and-delete as fallback
        let copy_opts = fs_extra::dir::CopyOptions::new().content_only(true);
        
        // Create target directory first for content_only copy
        fs::create_dir_all(&qpath)
            .with_context(|| format!("Failed to create quarantine directory {:?}", qpath))?;
        
        if let Err(copy_err) = fs_extra::dir::copy(target, &qpath, &copy_opts) {
            fs::remove_dir_all(&qpath).ok();
            return Err(anyhow::anyhow!(
                "Failed to move {:?} to quarantine (rename: {}, copy: {})", 
                target, e, copy_err
            ));
        }
        
        if let Err(rm_err) = fs::remove_dir_all(target) {
            fs::remove_dir_all(&qpath).ok();
            return Err(anyhow::anyhow!(
                "Failed to remove original {:?} after copy: {}", 
                target, rm_err
            ));
        }
    }
    
    // Compute SHA256 AFTER move (lazy - only if move succeeds)
    let checksum = match sha256_dir(&qpath) {
        Ok((hash, _)) => hash,
        Err(_) => "unknown".to_string(), // Don't fail on hash error
    };
    
    let rec = QuarantineRecord {
        id,
        original_path: target.to_string_lossy().to_string(),
        quarantine_path: qpath.to_string_lossy().to_string(),
        sha256: checksum,
        size_bytes: size,
        created_at: Utc::now(),
    };
    
    let mut list = read_index();
    list.push(rec.clone());
    write_index(&list)?;
    
    Ok(rec)
}

/// Move to quarantine with explicit skip of SHA256 (fastest option)
pub fn move_to_quarantine_fast(target: &Path) -> Result<QuarantineRecord> {
    let qdir = quarantine_dir();

    fs::create_dir_all(&qdir).ok();
    
    let id = format!("{}", Utc::now().timestamp_nanos_opt().unwrap_or(0));
    let qpath = qdir.join(format!("{}_{}", id, target.file_name().unwrap_or_default().to_string_lossy()));
    
    let size = quick_size(target);
    
    if let Err(e) = fs::rename(target, &qpath) {
        let copy_opts = fs_extra::dir::CopyOptions::new().content_only(true);
        fs::create_dir_all(&qpath)?;
        
        if let Err(copy_err) = fs_extra::dir::copy(target, &qpath, &copy_opts) {
            fs::remove_dir_all(&qpath).ok();
            return Err(anyhow::anyhow!(
                "Failed to quarantine {:?}: rename={}, copy={}", 
                target, e, copy_err
            ));
        }
        fs::remove_dir_all(target)?;
    }
    
    let rec = QuarantineRecord {
        id,
        original_path: target.to_string_lossy().to_string(),
        quarantine_path: qpath.to_string_lossy().to_string(),
        sha256: "deferred".to_string(), // Not computed
        size_bytes: size,
        created_at: Utc::now(),
    };
    
    let mut list = read_index();
    list.push(rec.clone());
    write_index(&list)?;
    
    Ok(rec)
}

#[allow(dead_code)]
pub fn list_quarantine() -> Vec<QuarantineRecord> { 
    read_index() 
}

pub fn latest_quarantine() -> Option<QuarantineRecord> {
    let mut list = read_index();
    list.sort_by_key(|r| r.created_at);
    list.pop()
}

pub fn find_quarantine_by_id(id: &str) -> Option<QuarantineRecord> {
    read_index().into_iter().find(|r| r.id == id)
}

pub fn rollback_record(rec: &QuarantineRecord) -> Result<()> {
    let orig = PathBuf::from(&rec.original_path);
    let q = PathBuf::from(&rec.quarantine_path);
    
    if let Some(parent) = orig.parent() { 
        fs::create_dir_all(parent).ok(); 
    }
    
    fs::rename(&q, &orig).with_context(|| {
        format!("Failed to rollback from quarantine: {:?} -> {:?}", q, orig)
    })?;
    
    // Remove from index
    let mut list = read_index();
    list.retain(|r| r.id != rec.id);
    write_index(&list)?;
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_config_default() {
        let config = QuarantineConfig::default();
        assert_eq!(config.max_size_gb, 10);
        assert_eq!(config.retention_days, 30);
        assert_eq!(config.max_entries, 200);
    }

    #[test]
    fn test_quick_size() {
        let temp = tempdir().unwrap();
        let test_file = temp.path().join("test.txt");
        fs::write(&test_file, "hello world").unwrap();
        
        let size = quick_size(temp.path());
        assert_eq!(size, 11); // "hello world".len()
    }

    #[test]
    fn test_sha256_dir() {
        let temp = tempdir().unwrap();
        fs::write(temp.path().join("a.txt"), "content a").unwrap();
        fs::write(temp.path().join("b.txt"), "content b").unwrap();
        
        let (hash, size) = sha256_dir(temp.path()).unwrap();
        assert!(!hash.is_empty());
        assert_eq!(size, 18); // 9 + 9
    }

    #[test]
    fn test_lazy_sha256() {
        let temp = tempdir().unwrap();
        fs::write(temp.path().join("test.txt"), "data").unwrap();
        
        // Create lazy closure
        let compute = sha256_dir_lazy(temp.path().to_path_buf());
        
        // Closure not yet executed
        // Execute and verify
        let (hash, size) = compute().unwrap();
        assert!(!hash.is_empty());
        assert_eq!(size, 4);
    }
}
