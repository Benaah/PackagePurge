use anyhow::{Context, Result};
use chrono::Utc;
use sha2::{Digest, Sha256};
use std::{fs, path::{Path, PathBuf}};

use crate::types::QuarantineRecord;

fn quarantine_dir() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".packagepurge").join("quarantine")
}

fn index_path() -> PathBuf {
    quarantine_dir().join("index.json")
}

fn read_index() -> Vec<QuarantineRecord> {
    let p = index_path();
    if let Ok(text) = fs::read_to_string(&p) {
        if let Ok(list) = serde_json::from_str::<Vec<QuarantineRecord>>(&text) { return list; }
    }
    Vec::new()
}

fn write_index(mut list: Vec<QuarantineRecord>) -> Result<()> {
    // keep only recent N entries (e.g., 200) to bound file size
    if list.len() > 200 { let keep = list.split_off(list.len() - 200); list = keep; }
    let data = serde_json::to_string_pretty(&list)?;
    fs::write(index_path(), data).context("Failed to write quarantine index")?;
    Ok(())
}

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

pub fn move_to_quarantine(target: &Path) -> Result<QuarantineRecord> {
    let qdir = quarantine_dir();
    fs::create_dir_all(&qdir).ok();
    let id = format!("{}", Utc::now().timestamp_nanos_opt().unwrap_or(0));
    let (checksum, size) = sha256_dir(target)?;
    let qpath = qdir.join(format!("{}_{}", id, target.file_name().unwrap_or_default().to_string_lossy()));
    if let Err(e) = fs::rename(target, &qpath) {
        // Handle cross-device link errors (os error 17 or 18 on Unix, or similar on Windows)
        // We simply try copy-and-delete as fallback for any rename failure
        if let Err(copy_err) = fs_extra::dir::copy(target, &qpath, &fs_extra::dir::CopyOptions::new().content_only(true)) {
             return Err(anyhow::anyhow!("Failed to move {:?} to quarantine (rename failed: {}, copy failed: {})", target, e, copy_err));
        }
        if let Err(rm_err) = fs::remove_dir_all(target) {
            // If we can't remove original, we should probably clean up the quarantine copy
            fs::remove_dir_all(&qpath).ok();
            return Err(anyhow::anyhow!("Failed to remove original {:?} after copy to quarantine: {}", target, rm_err));
        }
    }
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
    write_index(list)?;
    Ok(rec)
}

#[allow(dead_code)]
pub fn list_quarantine() -> Vec<QuarantineRecord> { read_index() }

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
    if let Some(parent) = orig.parent() { fs::create_dir_all(parent).ok(); }
    fs::rename(&q, &orig).with_context(|| "Failed to rollback from quarantine")?;
    // remove from index
    let mut list = read_index();
    list.retain(|r| r.id != rec.id);
    write_index(list)?;
    Ok(())
}
