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

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

use optimization::{plan_basic_cleanup, RulesConfig, OptimizationEngine};

#[derive(Parser)]
#[command(name = "packagepurge-core", version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Scan filesystem and output dependency/caches JSON
    Scan { #[arg(short, long)] paths: Vec<PathBuf> },
    /// Produce cleanup plan without mutating filesystem
    DryRun { #[arg(short, long, default_value_t = 90)] preserve_days: i64, #[arg(short, long)] paths: Vec<PathBuf> },
    /// Move targets to quarantine (atomic move) based on paths provided
    Quarantine { #[arg(required=true)] targets: Vec<PathBuf> },
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
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Scan { paths } => {
            let out = scanner::scan(&paths)?;
            println!("{}", serde_json::to_string_pretty(&out)?);
        }
        Commands::DryRun { preserve_days, paths } => {
            let scan = scanner::scan(&paths)?;
            let report = plan_basic_cleanup(&scan, &RulesConfig {
                preserve_days,
                enable_symlinking: false,
                enable_ml_prediction: false,
                lru_max_packages: 1000,
                lru_max_size_bytes: 10_000_000_000, // 10GB default
            })?;
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        Commands::Quarantine { targets } => {
            let mut recs = Vec::new();
            for t in targets {
                match safety::move_to_quarantine(&t) {
                    Ok(r) => recs.push(r),
                    Err(e) => eprintln!("Failed to quarantine {:?}: {}", t, e),
                }
            }
            println!("{}", serde_json::to_string_pretty(&recs)?);
        }
        Commands::Rollback { id, latest } => {
            let rec = if let Some(i) = id { safety::find_quarantine_by_id(&i) } else if latest { safety::latest_quarantine() } else { None };
            if let Some(r) = rec {
                if let Err(e) = safety::rollback_record(&r) {
                    eprintln!("{}", e);
                    std::process::exit(1);
                }
                println!("{}", serde_json::to_string_pretty(&serde_json::json!({"status":"ok","id": r.id}))?);
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
    }
    Ok(())
}
