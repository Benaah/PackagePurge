//! Incremental Scan Cache
//!
//! Caches scan results to enable faster subsequent scans by tracking:
//! - File modification times (mtime) to detect changes
//! - Package fingerprints for quick change detection
//! - Directory sizes to avoid redundant walks
//!
//! Expected improvement: 5-10x faster scans on subsequent runs.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;



/// Cached metadata for a single path
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedEntry {
    /// Last known modification time
    pub mtime: DateTime<Utc>,
    /// Quick fingerprint (mtime + size hash)
    pub fingerprint: String,
    /// Cached directory size (avoids walking)
    pub size_bytes: u64,
    /// When this cache entry was created
    pub cached_at: DateTime<Utc>,
}

/// Scan cache for incremental scanning
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScanCache {
    /// Path -> cached metadata
    entries: HashMap<String, CachedEntry>,
    /// When the cache was last saved
    pub last_saved: Option<DateTime<Utc>>,
    /// Cache version for migration
    pub version: u32,
}

impl ScanCache {
    const CURRENT_VERSION: u32 = 1;

    /// Create a new empty cache
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            last_saved: None,
            version: Self::CURRENT_VERSION,
        }
    }

    /// Load cache from disk, or create new if not exists
    pub fn load_or_create(cache_path: &Path) -> Result<Self> {

        if cache_path.exists() {
            let content = fs::read_to_string(cache_path)
                .with_context(|| format!("Failed to read scan cache from {:?}", cache_path))?;
            let cache: ScanCache = serde_json::from_str(&content)
                .with_context(|| "Failed to parse scan cache")?;
            
            // Check version compatibility
            if cache.version != Self::CURRENT_VERSION {
                eprintln!("Scan cache version mismatch, creating new cache");
                return Ok(Self::new());
            }
            
            Ok(cache)
        } else {
            Ok(Self::new())
        }
    }

    /// Persist cache to disk
    pub fn save(&mut self, cache_path: &Path) -> Result<()> {
        // Ensure parent directory exists
        if let Some(parent) = cache_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create cache directory {:?}", parent))?;
        }

        self.last_saved = Some(Utc::now());
        
        let content = serde_json::to_string_pretty(self)
            .with_context(|| "Failed to serialize scan cache")?;
        
        fs::write(cache_path, content)
            .with_context(|| format!("Failed to write scan cache to {:?}", cache_path))?;
        
        Ok(())
    }

    /// Get the default cache path
    pub fn default_cache_path() -> PathBuf {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        home.join(".packagepurge").join("scan_cache.json")
    }

    /// Generate fingerprint for a path based on mtime and file count
    fn generate_fingerprint(path: &Path) -> Result<(String, SystemTime, u64)> {
        let meta = fs::metadata(path)
            .with_context(|| format!("Failed to get metadata for {:?}", path))?;
        
        let mtime = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        
        // Quick fingerprint: mtime timestamp + direct children count
        let mut hasher_input = format!("{:?}", mtime);
        let mut size: u64 = 0;
        
        if path.is_dir() {
            // Only count immediate children for fingerprint (fast)
            if let Ok(entries) = fs::read_dir(path) {
                let count = entries.count();
                hasher_input.push_str(&format!(":children={}", count));
            }
            
            // For package.json mtime (if exists)
            let pkg_json = path.join("package.json");
            if let Ok(pkg_meta) = fs::metadata(&pkg_json) {
                if let Ok(pkg_mtime) = pkg_meta.modified() {
                    hasher_input.push_str(&format!(":pkg={:?}", pkg_mtime));
                }
                size = pkg_meta.len();
            }
        } else {
            size = meta.len();
        }
        
        // Simple hash of the fingerprint string
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(hasher_input.as_bytes());
        let fingerprint = hex::encode(&hasher.finalize()[..8]);
        
        Ok((fingerprint, mtime, size))
    }

    /// Check if a path is stale (needs re-scanning)
    pub fn is_stale(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy().to_string();
        
        match self.entries.get(&path_str) {
            None => true, // Not in cache
            Some(cached) => {
                // Check if file still exists
                if !path.exists() {
                    return true;
                }
                
                // Quick mtime check first
                if let Ok(meta) = fs::metadata(path) {
                    if let Ok(current_mtime) = meta.modified() {
                        let current_utc: DateTime<Utc> = current_mtime.into();
                        if current_utc != cached.mtime {
                            return true;
                        }
                    }
                }
                
                // Fingerprint check for deeper validation
                if let Ok((fingerprint, _, _)) = Self::generate_fingerprint(path) {
                    return fingerprint != cached.fingerprint;
                }
                
                true // If we can't verify, assume stale
            }
        }
    }

    /// Update cache entry for a path with pre-computed size
    pub fn update(&mut self, path: &Path, size_bytes: u64) -> Result<()> {
        let path_str = path.to_string_lossy().to_string();
        let (fingerprint, mtime, _) = Self::generate_fingerprint(path)?;
        
        self.entries.insert(path_str, CachedEntry {
            mtime: mtime.into(),
            fingerprint,
            size_bytes,
            cached_at: Utc::now(),
        });
        
        Ok(())
    }

    /// Get cached size for a path (None if stale or not cached)
    pub fn get_cached_size(&self, path: &Path) -> Option<u64> {
        if self.is_stale(path) {
            return None;
        }
        
        let path_str = path.to_string_lossy().to_string();
        self.entries.get(&path_str).map(|e| e.size_bytes)
    }

    /// Get cached package record if still valid
    pub fn get_cached_package(&self, path: &Path) -> Option<&CachedEntry> {
        let path_str = path.to_string_lossy().to_string();
        
        if self.is_stale(path) {
            return None;
        }
        
        self.entries.get(&path_str)
    }

    /// Remove entries for paths that no longer exist
    pub fn prune_missing(&mut self) {
        self.entries.retain(|path_str, _| {
            Path::new(path_str).exists()
        });
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            total_entries: self.entries.len(),
            total_cached_size: self.entries.values().map(|e| e.size_bytes).sum(),
            last_saved: self.last_saved,
        }
    }

    /// Clear the cache
    pub fn clear(&mut self) {
        self.entries.clear();
        self.last_saved = None;
    }
}

/// Cache statistics for reporting
#[derive(Debug, Clone, Serialize)]
pub struct CacheStats {
    pub total_entries: usize,
    pub total_cached_size: u64,
    pub last_saved: Option<DateTime<Utc>>,
}

/// Wrapper for scanning with cache support
pub struct CachedScanner {
    cache: ScanCache,
    cache_path: PathBuf,
    hits: usize,
    misses: usize,
}

impl CachedScanner {
    /// Create a new cached scanner
    pub fn new() -> Result<Self> {
        let cache_path = ScanCache::default_cache_path();
        let cache = ScanCache::load_or_create(&cache_path)?;
        
        Ok(Self {
            cache,
            cache_path,
            hits: 0,
            misses: 0,
        })
    }

    /// Create with custom cache path
    pub fn with_cache_path(cache_path: PathBuf) -> Result<Self> {
        let cache = ScanCache::load_or_create(&cache_path)?;
        
        Ok(Self {
            cache,
            cache_path,
            hits: 0,
            misses: 0,
        })
    }

    /// Get cached size or compute it
    pub fn get_or_compute_size<F>(&mut self, path: &Path, compute: F) -> u64
    where
        F: FnOnce() -> u64,
    {
        if let Some(size) = self.cache.get_cached_size(path) {
            self.hits += 1;
            size
        } else {
            self.misses += 1;
            let size = compute();
            let _ = self.cache.update(path, size);
            size
        }
    }

    /// Persist cache to disk
    pub fn save(&mut self) -> Result<()> {
        self.cache.save(&self.cache_path)
    }

    /// Get hit/miss statistics
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }

    /// Get cache statistics
    pub fn stats(&self) -> (CacheStats, usize, usize) {
        (self.cache.stats(), self.hits, self.misses)
    }
}

impl Default for CachedScanner {
    fn default() -> Self {
        Self::new().unwrap_or_else(|_| Self {
            cache: ScanCache::new(),
            cache_path: ScanCache::default_cache_path(),
            hits: 0,
            misses: 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_cache_new() {
        let cache = ScanCache::new();
        assert_eq!(cache.entries.len(), 0);
        assert_eq!(cache.version, ScanCache::CURRENT_VERSION);
    }

    #[test]
    fn test_cache_save_load() {
        let temp = tempdir().unwrap();
        let cache_path = temp.path().join("test_cache.json");
        
        let mut cache = ScanCache::new();
        cache.update(temp.path(), 1234).unwrap();
        cache.save(&cache_path).unwrap();
        
        let loaded = ScanCache::load_or_create(&cache_path).unwrap();
        assert_eq!(loaded.entries.len(), 1);
    }

    #[test]
    fn test_cache_staleness() {
        let cache = ScanCache::new();
        let temp = tempdir().unwrap();
        
        // Non-existent path should be stale
        assert!(cache.is_stale(Path::new("/nonexistent/path")));
        
        // Existing uncached path should be stale
        assert!(cache.is_stale(temp.path()));
    }

    #[test]
    fn test_cached_scanner() {
        let temp = tempdir().unwrap();
        let cache_path = temp.path().join("scanner_cache.json");
        
        let mut scanner = CachedScanner::with_cache_path(cache_path).unwrap();
        
        // First call should be a miss
        let size1 = scanner.get_or_compute_size(temp.path(), || 100);
        assert_eq!(size1, 100);
        
        // Second call should be a hit (same size returned)
        let size2 = scanner.get_or_compute_size(temp.path(), || 200);
        assert_eq!(size2, 100); // Cached value, not new computation
        
        assert_eq!(scanner.hits, 1);
        assert_eq!(scanner.misses, 1);
    }
}
