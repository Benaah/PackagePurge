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

/// Usage metrics for a package
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageUsageMetrics {
    pub package_key: String, // Format: "name@version"
    pub last_access_time: DateTime<Utc>,
    pub last_script_execution: Option<DateTime<Utc>>,
    pub access_count: u64,
    pub script_execution_count: u64,
    pub last_successful_build: Option<DateTime<Utc>>,
}

impl Default for PackageUsageMetrics {
    fn default() -> Self {
        Self {
            package_key: String::new(),
            last_access_time: Utc::now(),
            last_script_execution: None,
            access_count: 0,
            script_execution_count: 0,
            last_successful_build: None,
        }
    }
}


/// Project metadata for ML features
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectMetadata {
    pub path: String,
    pub project_type: String, // e.g., "react", "node", "typescript"
    pub last_commit_date: Option<DateTime<Utc>>,
    pub dependency_count: usize,
    pub last_modified: DateTime<Utc>,
}

/// Developer behavior metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeveloperBehavior {
    pub npm_commands_executed: Vec<(String, DateTime<Utc>)>, // (command, timestamp)
    pub file_access_frequency: u64,
    pub days_since_last_build: Option<i64>,
}