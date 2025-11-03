use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PackageManager { Npm, Yarn, Pnpm }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageRecord {
    pub name: String,
    pub version: String,
    pub path: String,
    pub size_bytes: u64,
    pub atime: DateTime<Utc>,
    pub mtime: DateTime<Utc>,
    pub manager: Option<PackageManager>,
    pub project_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectRecord {
    pub path: String,
    pub manager: Option<PackageManager>,
    pub dependencies: Vec<(String, String)>,
    pub mtime: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanOutput {
    pub packages: Vec<PackageRecord>,
    pub projects: Vec<ProjectRecord>,
    pub edges: Vec<(String, String)>, // parent -> dependency
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanItem {
    pub target_path: String,
    pub estimated_size_bytes: u64,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DryRunReport {
    pub items: Vec<PlanItem>,
    pub total_estimated_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuarantineRecord {
    pub id: String,
    pub original_path: String,
    pub quarantine_path: String,
    pub sha256: String,
    pub size_bytes: u64,
    pub created_at: DateTime<Utc>,
}
