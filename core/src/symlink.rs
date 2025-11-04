use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

#[cfg(windows)]
use std::os::windows::fs as win_fs;

#[cfg(unix)]
use std::os::unix::fs as unix_fs;

/// Global store path (platform-specific)
pub fn get_global_store_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Could not determine home directory")?;
    Ok(home.join(".packagepurge").join("global_store"))
}

/// Initialize global store directory
pub fn ensure_global_store() -> Result<PathBuf> {
    let store_path = get_global_store_path()?;
    fs::create_dir_all(&store_path)
        .with_context(|| format!("Failed to create global store at {:?}", store_path))?;
    Ok(store_path)
}

/// Generate content-addressable path for a package
/// Format: global_store/{name}/{version}/{hash}
pub fn get_canonical_path(store_path: &Path, name: &str, version: &str) -> Result<PathBuf> {
    // Use a simple hash of name@version for content addressing
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(format!("{}@{}", name, version).as_bytes());
    let hash = hex::encode(&hasher.finalize()[..8]);
    
    Ok(store_path
        .join(sanitize_name(name))
        .join(version)
        .join(&hash))
}

fn sanitize_name(name: &str) -> String {
    name.replace("/", "_").replace("\\", "_").replace(":", "_")
}

/// Check if a path is a symlink (or junction on Windows)
pub fn is_symlink(path: &Path) -> bool {
    #[cfg(windows)]
    {
        // On Windows, try to read the link - if it succeeds, it's a symlink
        if fs::read_link(path).is_ok() {
            return true;
        }
        // Also check metadata for symlink file type
        if let Ok(meta) = fs::symlink_metadata(path) {
            return meta.file_type().is_symlink();
        }
        false
    }
    
    #[cfg(unix)]
    {
        if let Ok(meta) = fs::symlink_metadata(path) {
            meta.file_type().is_symlink()
        } else {
            false
        }
    }
}

/// Create hard links for all files in source directory to target directory
pub fn hard_link_directory(src: &Path, dst: &Path) -> Result<()> {
    if dst.exists() {
        fs::remove_dir_all(dst)
            .with_context(|| format!("Failed to remove existing directory {:?}", dst))?;
    }
    fs::create_dir_all(dst)
        .with_context(|| format!("Failed to create directory {:?}", dst))?;

    // Recursively hard link all files
    copy_directory_with_hard_links(src, dst)?;
    Ok(())
}

fn copy_directory_with_hard_links(src: &Path, dst: &Path) -> Result<()> {
    use walkdir::WalkDir;
    
    for entry in WalkDir::new(src).into_iter().filter_map(|e| e.ok()) {
        let src_path = entry.path();
        let rel_path = src_path.strip_prefix(src)
            .with_context(|| format!("Failed to get relative path from {:?}", src))?;
        let dst_path = dst.join(rel_path);

        if src_path.is_dir() {
            fs::create_dir_all(&dst_path)
                .with_context(|| format!("Failed to create directory {:?}", dst_path))?;
        } else if src_path.is_file() {
            if let Some(parent) = dst_path.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("Failed to create parent directory {:?}", parent))?;
            }
            
            #[cfg(unix)]
            {
                fs::hard_link(src_path, &dst_path)
                    .with_context(|| format!("Failed to create hard link from {:?} to {:?}", src_path, dst_path))?;
            }
            
            #[cfg(windows)]
            {
                // Windows: try hard link first, fall back to copy
                if fs::hard_link(src_path, &dst_path).is_err() {
                    // If hard link fails (e.g., different volumes), copy the file
                    fs::copy(src_path, &dst_path)
                        .with_context(|| format!("Failed to copy file from {:?} to {:?}", src_path, dst_path))?;
                }
            }
        }
    }
    Ok(())
}

/// Create a symlink (or junction on Windows) from target to source
pub fn create_symlink(target: &Path, source: &Path) -> Result<()> {
    // Remove existing target if it exists
    if target.exists() {
        if target.is_dir() {
            fs::remove_dir_all(target)
                .with_context(|| format!("Failed to remove existing directory {:?}", target))?;
        } else {
            fs::remove_file(target)
                .with_context(|| format!("Failed to remove existing file {:?}", target))?;
        }
    }
    
    // Ensure parent directory exists
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create parent directory {:?}", parent))?;
    }

    #[cfg(windows)]
    {
        // On Windows, use directory symlink for directories, symlink for files
        if source.is_dir() {
            // Try to create a directory symlink (requires admin privileges or Developer Mode)
            win_fs::symlink_dir(source, target)
                .with_context(|| format!("Failed to create directory symlink from {:?} to {:?}. Note: On Windows, this may require administrator privileges or Developer Mode enabled.", target, source))?;
        } else {
            win_fs::symlink_file(source, target)
                .with_context(|| format!("Failed to create file symlink from {:?} to {:?}", target, source))?;
        }
    }
    
    #[cfg(unix)]
    {
        unix_fs::symlink(source, target)
            .with_context(|| format!("Failed to create symlink from {:?} to {:?}", target, source))?;
    }
    
    Ok(())
}

/// Deduplicate packages by creating symlinks to global store
#[allow(dead_code)]
pub struct SemanticDeduplication {
    store_path: PathBuf,
}

impl SemanticDeduplication {
    pub fn new() -> Result<Self> {
        let store_path = ensure_global_store()?;
        Ok(Self { store_path })
    }

    /// Process a package: hard link to global store, then symlink from original location
    pub fn deduplicate_package(&self, package_path: &Path, name: &str, version: &str) -> Result<()> {
        let canonical_path = get_canonical_path(&self.store_path, name, version)?;
        
        // If canonical doesn't exist, create it by hard linking from package_path
        if !canonical_path.exists() {
            hard_link_directory(package_path, &canonical_path)
                .with_context(|| format!("Failed to create canonical package at {:?}", canonical_path))?;
        }
        
        // If package_path is not already a symlink, replace it with one
        if !is_symlink(package_path) {
            // Create a temporary path for safe replacement
            let temp_path = package_path.with_extension(".packagepurge.tmp");
            
            // Create symlink at temp location first
            create_symlink(&temp_path, &canonical_path)?;
            
            // Remove original and rename temp
            if package_path.is_dir() {
                fs::remove_dir_all(package_path)
                    .with_context(|| format!("Failed to remove original directory {:?}", package_path))?;
            } else {
                fs::remove_file(package_path)
                    .with_context(|| format!("Failed to remove original file {:?}", package_path))?;
            }
            
            fs::rename(&temp_path, package_path)
                .with_context(|| format!("Failed to rename temp symlink to {:?}", package_path))?;
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_get_canonical_path() {
        let store = PathBuf::from("/tmp/store");
        let path = get_canonical_path(&store, "react", "18.2.0").unwrap();
        assert!(path.to_string_lossy().contains("react"));
        assert!(path.to_string_lossy().contains("18.2.0"));
    }
}

