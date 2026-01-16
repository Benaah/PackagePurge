//! Feature Store - SQLite-backed persistence for ML features
//!
//! Provides persistent storage for:
//! - Package usage metrics
//! - Project metadata
//! - Developer behavior patterns
//! - ML feature vectors
//!
//! This replaces JSON file storage with SQLite for better performance and querying.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::{Path, PathBuf};

use crate::types::{PackageUsageMetrics, ProjectMetadata};

/// SQLite-backed feature store
pub struct FeatureStore {
    conn: Connection,
}

impl FeatureStore {
    /// Default path for the feature store database
    pub fn default_db_path() -> PathBuf {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        home.join(".packagepurge").join("features.db")
    }

    /// Open or create a feature store at the given path
    pub fn open(db_path: &Path) -> Result<Self> {
        // Ensure parent directory exists
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory {:?}", parent))?;
        }

        let conn = Connection::open(db_path)
            .with_context(|| format!("Failed to open database at {:?}", db_path))?;

        let store = Self { conn };
        store.initialize_schema()?;
        
        Ok(store)
    }

    /// Open the default feature store
    pub fn open_default() -> Result<Self> {
        Self::open(&Self::default_db_path())
    }

    /// Initialize database schema
    fn initialize_schema(&self) -> Result<()> {
        self.conn.execute_batch(r#"
            -- Package usage metrics
            CREATE TABLE IF NOT EXISTS package_metrics (
                package_key TEXT PRIMARY KEY,
                last_access_time TEXT NOT NULL,
                last_script_execution TEXT,
                access_count INTEGER NOT NULL DEFAULT 0,
                script_execution_count INTEGER NOT NULL DEFAULT 0,
                last_successful_build TEXT,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );

            -- Project metadata
            CREATE TABLE IF NOT EXISTS projects (
                path TEXT PRIMARY KEY,
                project_type TEXT,
                last_commit_date TEXT,
                dependency_count INTEGER NOT NULL DEFAULT 0,
                last_modified TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );

            -- Developer behavior patterns
            CREATE TABLE IF NOT EXISTS behavior_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                event_type TEXT NOT NULL,
                command TEXT,
                project_path TEXT,
                timestamp TEXT NOT NULL,
                metadata TEXT
            );

            -- ML feature vectors (pre-computed for inference)
            CREATE TABLE IF NOT EXISTS feature_vectors (
                package_key TEXT PRIMARY KEY,
                feature_version INTEGER NOT NULL DEFAULT 1,
                features BLOB NOT NULL,
                computed_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );

            -- Indexes for common queries
            CREATE INDEX IF NOT EXISTS idx_package_metrics_access 
                ON package_metrics(last_access_time);
            CREATE INDEX IF NOT EXISTS idx_behavior_events_timestamp 
                ON behavior_events(timestamp);
            CREATE INDEX IF NOT EXISTS idx_projects_modified 
                ON projects(last_modified);
        "#).context("Failed to initialize database schema")?;

        Ok(())
    }

    // =========================================================================
    // Package Metrics
    // =========================================================================

    /// Record or update package access
    pub fn record_package_access(&self, package_key: &str, _size_bytes: u64) -> Result<()> {

        let now = Utc::now().to_rfc3339();
        
        self.conn.execute(
            r#"
            INSERT INTO package_metrics (package_key, last_access_time, access_count)
            VALUES (?1, ?2, 1)
            ON CONFLICT(package_key) DO UPDATE SET
                last_access_time = ?2,
                access_count = access_count + 1,
                updated_at = ?2
            "#,
            params![package_key, now],
        ).context("Failed to record package access")?;
        
        Ok(())
    }

    /// Record script execution for a package
    pub fn record_script_execution(&self, package_key: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        
        self.conn.execute(
            r#"
            UPDATE package_metrics SET
                last_script_execution = ?2,
                script_execution_count = script_execution_count + 1,
                updated_at = ?2
            WHERE package_key = ?1
            "#,
            params![package_key, now],
        ).context("Failed to record script execution")?;
        
        Ok(())
    }

    /// Record successful build for a package
    pub fn record_build(&self, package_key: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        
        self.conn.execute(
            r#"
            UPDATE package_metrics SET
                last_successful_build = ?2,
                updated_at = ?2
            WHERE package_key = ?1
            "#,
            params![package_key, now],
        ).context("Failed to record build")?;
        
        Ok(())
    }

    /// Get metrics for a package
    pub fn get_package_metrics(&self, package_key: &str) -> Result<Option<PackageUsageMetrics>> {
        let result = self.conn.query_row(
            r#"
            SELECT package_key, last_access_time, last_script_execution, 
                   access_count, script_execution_count, last_successful_build
            FROM package_metrics WHERE package_key = ?1
            "#,
            params![package_key],
            |row| {
                let package_key: String = row.get(0)?;
                let last_access_str: String = row.get(1)?;
                let last_script_str: Option<String> = row.get(2)?;
                let access_count: u64 = row.get(3)?;
                let script_count: u64 = row.get(4)?;
                let last_build_str: Option<String> = row.get(5)?;
                
                Ok((package_key, last_access_str, last_script_str, access_count, script_count, last_build_str))
            },
        ).optional().context("Failed to query package metrics")?;

        match result {
            Some((key, access_str, script_str, access_count, script_count, build_str)) => {
                let last_access = DateTime::parse_from_rfc3339(&access_str)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now());
                
                let last_script = script_str.and_then(|s| 
                    DateTime::parse_from_rfc3339(&s).ok().map(|dt| dt.with_timezone(&Utc))
                );
                
                let last_build = build_str.and_then(|s| 
                    DateTime::parse_from_rfc3339(&s).ok().map(|dt| dt.with_timezone(&Utc))
                );

                Ok(Some(PackageUsageMetrics {
                    package_key: key,
                    last_access_time: last_access,
                    last_script_execution: last_script,
                    access_count,
                    script_execution_count: script_count,
                    last_successful_build: last_build,
                }))
            }
            None => Ok(None),
        }
    }

    /// Get packages not accessed in the last N days
    pub fn get_stale_packages(&self, days: i64) -> Result<Vec<String>> {
        let cutoff = (Utc::now() - chrono::Duration::days(days)).to_rfc3339();
        
        let mut stmt = self.conn.prepare(
            "SELECT package_key FROM package_metrics WHERE last_access_time < ?1"
        )?;
        
        let packages = stmt.query_map(params![cutoff], |row| row.get(0))?
            .collect::<std::result::Result<Vec<String>, _>>()
            .context("Failed to get stale packages")?;
        
        Ok(packages)
    }

    /// Get top N most accessed packages
    pub fn get_top_packages(&self, limit: usize) -> Result<Vec<(String, u64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT package_key, access_count FROM package_metrics 
             ORDER BY access_count DESC LIMIT ?1"
        )?;
        
        let packages = stmt.query_map(params![limit as i64], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()
        .context("Failed to get top packages")?;
        
        Ok(packages)
    }

    // =========================================================================
    // Projects
    // =========================================================================

    /// Upsert project metadata
    pub fn upsert_project(&self, project: &ProjectMetadata) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let last_modified = project.last_modified.to_rfc3339();
        let last_commit = project.last_commit_date.map(|d| d.to_rfc3339());
        
        self.conn.execute(
            r#"
            INSERT INTO projects (path, project_type, last_commit_date, dependency_count, last_modified, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(path) DO UPDATE SET
                project_type = ?2,
                last_commit_date = ?3,
                dependency_count = ?4,
                last_modified = ?5,
                updated_at = ?6
            "#,
            params![project.path, project.project_type, last_commit, project.dependency_count as i64, last_modified, now],
        ).context("Failed to upsert project")?;
        
        Ok(())
    }

    // =========================================================================
    // Behavior Events
    // =========================================================================

    /// Log a developer behavior event
    pub fn log_event(&self, event_type: &str, command: Option<&str>, project_path: Option<&str>) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        
        self.conn.execute(
            "INSERT INTO behavior_events (event_type, command, project_path, timestamp) VALUES (?1, ?2, ?3, ?4)",
            params![event_type, command, project_path, now],
        ).context("Failed to log event")?;
        
        Ok(())
    }

    // =========================================================================
    // Feature Vectors
    // =========================================================================

    /// Store pre-computed feature vector for a package
    pub fn store_features(&self, package_key: &str, features: &[f64]) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let blob: Vec<u8> = features.iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();
        
        self.conn.execute(
            r#"
            INSERT INTO feature_vectors (package_key, features, computed_at)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(package_key) DO UPDATE SET
                features = ?2,
                computed_at = ?3
            "#,
            params![package_key, blob, now],
        ).context("Failed to store features")?;
        
        Ok(())
    }

    /// Get feature vector for a package
    pub fn get_features(&self, package_key: &str) -> Result<Option<Vec<f64>>> {
        let blob: Option<Vec<u8>> = self.conn.query_row(
            "SELECT features FROM feature_vectors WHERE package_key = ?1",
            params![package_key],
            |row| row.get(0),
        ).optional().context("Failed to get features")?;

        match blob {
            Some(bytes) => {
                let features: Vec<f64> = bytes.chunks(8)
                    .map(|chunk| {
                        let arr: [u8; 8] = chunk.try_into().unwrap_or([0; 8]);
                        f64::from_le_bytes(arr)
                    })
                    .collect();
                Ok(Some(features))
            }
            None => Ok(None),
        }
    }

    // =========================================================================
    // Maintenance
    // =========================================================================

    /// Get database statistics
    pub fn get_stats(&self) -> Result<FeatureStoreStats> {
        let package_count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM package_metrics", [], |row| row.get(0)
        )?;
        
        let project_count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM projects", [], |row| row.get(0)
        )?;
        
        let event_count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM behavior_events", [], |row| row.get(0)
        )?;
        
        let feature_count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM feature_vectors", [], |row| row.get(0)
        )?;

        Ok(FeatureStoreStats {
            package_count: package_count as usize,
            project_count: project_count as usize,
            event_count: event_count as usize,
            feature_count: feature_count as usize,
        })
    }

    /// Vacuum the database to reclaim space
    pub fn vacuum(&self) -> Result<()> {
        self.conn.execute("VACUUM", []).context("Failed to vacuum database")?;
        Ok(())
    }

    /// Prune old events (keep last N days)
    pub fn prune_old_events(&self, keep_days: i64) -> Result<usize> {
        let cutoff = (Utc::now() - chrono::Duration::days(keep_days)).to_rfc3339();
        
        let deleted = self.conn.execute(
            "DELETE FROM behavior_events WHERE timestamp < ?1",
            params![cutoff],
        ).context("Failed to prune old events")?;
        
        Ok(deleted)
    }
}

/// Statistics about the feature store
#[derive(Debug, Clone, serde::Serialize)]
pub struct FeatureStoreStats {
    pub package_count: usize,
    pub project_count: usize,
    pub event_count: usize,
    pub feature_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_open_and_schema() {
        let temp = tempdir().unwrap();
        let db_path = temp.path().join("test.db");
        
        let store = FeatureStore::open(&db_path).unwrap();
        let stats = store.get_stats().unwrap();
        
        assert_eq!(stats.package_count, 0);
        assert_eq!(stats.project_count, 0);
    }

    #[test]
    fn test_package_access() {
        let temp = tempdir().unwrap();
        let db_path = temp.path().join("test.db");
        
        let store = FeatureStore::open(&db_path).unwrap();
        
        // Record access
        store.record_package_access("lodash@4.17.21", 1024).unwrap();
        store.record_package_access("lodash@4.17.21", 1024).unwrap();
        
        // Get metrics
        let metrics = store.get_package_metrics("lodash@4.17.21").unwrap().unwrap();
        assert_eq!(metrics.package_key, "lodash@4.17.21");
        assert_eq!(metrics.access_count, 2);
    }

    #[test]
    fn test_stale_packages() {
        let temp = tempdir().unwrap();
        let db_path = temp.path().join("test.db");
        
        let store = FeatureStore::open(&db_path).unwrap();
        store.record_package_access("recent-pkg@1.0.0", 1024).unwrap();
        
        // Should not be stale (accessed just now)
        let stale = store.get_stale_packages(30).unwrap();
        assert!(stale.is_empty());
    }

    #[test]
    fn test_feature_vectors() {
        let temp = tempdir().unwrap();
        let db_path = temp.path().join("test.db");
        
        let store = FeatureStore::open(&db_path).unwrap();
        
        let features = vec![1.0, 2.5, 3.7, 0.0, -1.5];
        store.store_features("test-pkg@1.0.0", &features).unwrap();
        
        let loaded = store.get_features("test-pkg@1.0.0").unwrap().unwrap();
        assert_eq!(loaded.len(), features.len());
        for (a, b) in loaded.iter().zip(features.iter()) {
            assert!((a - b).abs() < 0.0001);
        }
    }
}
