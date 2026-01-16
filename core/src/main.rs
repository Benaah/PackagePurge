mod types;
mod scanner;
mod safety;
mod optimization;
mod cache;
mod ml;
mod arc_lfu;
mod lockfiles;
mod symlink;
mod usage_tracker;
mod scan_cache;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

use optimization::{plan_basic_cleanup, RulesConfig, OptimizationEngine};
use safety::{get_quarantine_stats, cleanup_quarantine, save_config};
use scan_cache::ScanCache;

#[derive(Parser)]
#[command(name = "packagepurge-core", version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Scan filesystem and output dependency/caches JSON
    Scan { 
        #[arg(short, long)] 
        paths: Vec<PathBuf>,
        /// Skip cache (force fresh scan)
        #[arg(long)]
        no_cache: bool,
    },
    /// Produce cleanup plan without mutating filesystem
    DryRun { 
        #[arg(short, long, default_value_t = 90)] 
        preserve_days: i64, 
        #[arg(short, long)] 
        paths: Vec<PathBuf> 
    },
    /// Move targets to quarantine (atomic move) based on paths provided
    Quarantine { 
        #[arg(required=true)] 
        targets: Vec<PathBuf>,
        /// Skip SHA256 verification for faster cleanup
        #[arg(long)]
        fast: bool,
    },
    /// Rollback by id or latest
    Rollback {
        #[arg(long)] id: Option<String>,
        #[arg(long)] latest: bool,
    },
    /// Optimize with ML/LRU and symlinking (dry run)
    Optimize {
        #[arg(short, long, default_value_t = 90)] preserve_days: i64,
        #[arg(short, long)] paths: Vec<PathBuf>,
        #[arg(long)] enable_symlinking: bool,
        #[arg(long)] enable_ml: bool,
        #[arg(long, default_value_t = 1000)] lru_max_packages: usize,
        #[arg(long, default_value_t = 10_000_000_000)] lru_max_size_bytes: u64,
    },
    /// Execute symlinking for duplicate packages
    Symlink {
        #[arg(short, long)] paths: Vec<PathBuf>,
    },
    /// Show statistics about quarantine and cache
    Stats,
    /// Cleanup old quarantine entries based on retention policy
    CleanupQuarantine {
        /// Maximum quarantine size in GB
        #[arg(long)]
        max_size_gb: Option<u64>,
        /// Days to retain quarantine entries
        #[arg(long)]
        retention_days: Option<i64>,
    },
    /// Clear the scan cache (force fresh scans)
    ClearCache,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Scan { paths, no_cache } => {
            let out = if no_cache {
                scanner::scan_no_cache(&paths)?
            } else {
                scanner::scan(&paths)?
            };
            println!("{}", serde_json::to_string_pretty(&out)?);
        }
        Commands::DryRun { preserve_days, paths } => {
            let scan = scanner::scan(&paths)?;
            let report = plan_basic_cleanup(&scan, &RulesConfig {
                preserve_days,
                enable_symlinking: false,
                enable_ml_prediction: false,
                lru_max_packages: 1000,
                lru_max_size_bytes: 10_000_000_000,
            })?;
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        Commands::Quarantine { targets, fast } => {
            let mut recs = Vec::new();
            for t in targets {
                let result = if fast {
                    safety::move_to_quarantine_fast(&t)
                } else {
                    safety::move_to_quarantine(&t)
                };
                match result {
                    Ok(r) => recs.push(r),
                    Err(e) => eprintln!("Failed to quarantine {:?}: {}", t, e),
                }
            }
            println!("{}", serde_json::to_string_pretty(&recs)?);
        }
        Commands::Rollback { id, latest } => {
            let rec = if let Some(i) = id { 
                safety::find_quarantine_by_id(&i) 
            } else if latest { 
                safety::latest_quarantine() 
            } else { 
                None 
            };
            if let Some(r) = rec {
                if let Err(e) = safety::rollback_record(&r) {
                    eprintln!("{}", e);
                    std::process::exit(1);
                }
                println!("{}", serde_json::to_string_pretty(&serde_json::json!({
                    "status": "ok",
                    "id": r.id
                }))?);
            } else {
                eprintln!("No matching quarantine record found");
                std::process::exit(2);
            }
        }
        Commands::Optimize { preserve_days, paths, enable_symlinking, enable_ml, lru_max_packages, lru_max_size_bytes } => {
            let scan = scanner::scan(&paths)?;
            let config = RulesConfig {
                preserve_days,
                enable_symlinking,
                enable_ml_prediction: enable_ml,
                lru_max_packages,
                lru_max_size_bytes,
            };
            let mut engine = OptimizationEngine::new(config)?;
            let report = engine.plan_optimized_cleanup(&scan)?;
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        Commands::Symlink { paths } => {
            let scan = scanner::scan(&paths)?;
            let config = RulesConfig {
                preserve_days: 90,
                enable_symlinking: true,
                enable_ml_prediction: false,
                lru_max_packages: 1000,
                lru_max_size_bytes: 10_000_000_000,
            };
            let engine = OptimizationEngine::new(config)?;
            let count = engine.execute_symlinking(&scan)?;
            println!("{}", serde_json::to_string_pretty(&serde_json::json!({
                "status": "ok",
                "symlinked_count": count
            }))?);
        }
        Commands::Stats => {
            let q_stats = get_quarantine_stats();
            let cache_path = ScanCache::default_cache_path();
            let cache_stats = if cache_path.exists() {
                ScanCache::load_or_create(&cache_path)
                    .map(|c| c.stats())
                    .ok()
            } else {
                None
            };
            
            println!("{}", serde_json::to_string_pretty(&serde_json::json!({
                "quarantine": {
                    "total_entries": q_stats.total_entries,
                    "total_size_bytes": q_stats.total_size_bytes,
                    "oldest_entry_days": q_stats.oldest_entry_days,
                    "entries_over_retention": q_stats.entries_over_retention,
                },
                "scan_cache": cache_stats.map(|s| serde_json::json!({
                    "total_entries": s.total_entries,
                    "total_cached_size": s.total_cached_size,
                    "last_saved": s.last_saved,
                })),
            }))?);
        }
        Commands::CleanupQuarantine { max_size_gb, retention_days } => {
            // Update config if parameters provided
            if max_size_gb.is_some() || retention_days.is_some() {
                let mut config = safety::load_config();
                if let Some(size) = max_size_gb {
                    config.max_size_gb = size;
                }
                if let Some(days) = retention_days {
                    config.retention_days = days;
                }
                save_config(&config)?;
            }
            
            let (cleaned, bytes_freed) = cleanup_quarantine()?;
            println!("{}", serde_json::to_string_pretty(&serde_json::json!({
                "status": "ok",
                "entries_cleaned": cleaned,
                "bytes_freed": bytes_freed,
            }))?);
        }
        Commands::ClearCache => {
            let cache_path = ScanCache::default_cache_path();
            if cache_path.exists() {
                std::fs::remove_file(&cache_path)?;
                println!("{}", serde_json::to_string_pretty(&serde_json::json!({
                    "status": "ok",
                    "message": "Scan cache cleared"
                }))?);
            } else {
                println!("{}", serde_json::to_string_pretty(&serde_json::json!({
                    "status": "ok",
                    "message": "No cache to clear"
                }))?);
            }
        }
    }
    Ok(())
}
