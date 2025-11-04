# Phase 2 & 3 Implementation Summary

## Overview

This document summarizes the implementation of **Cross-Project Symlinking** (Step 4) and **LRU/ML-Driven Caching** (Step 5) for PackagePurge.

## Step 4: Cross-Project Symlinking

### Implementation Location
- **Module**: `core/src/symlink.rs`
- **Integration**: `core/src/optimization.rs` (OptimizationEngine)

### Key Components

#### 1. Global Store (`~/.packagepurge/global_store/`)
- **Location**: Platform-specific home directory
- **Structure**: Content-addressable storage (CAS) using format: `{name}/{version}/{hash}/`
- **Initialization**: `ensure_global_store()` creates the directory structure

#### 2. Hard Linking
- **Function**: `hard_link_directory(src, dst)`
- **Behavior**: 
  - Creates hard links for all files in source directory
  - On Windows: Falls back to file copy if hard linking fails (e.g., different volumes)
  - On Unix: Uses standard `fs::hard_link()`
- **Purpose**: Single data instance on disk, multiple file paths reference it

#### 3. Symlink Creation
- **Function**: `create_symlink(target, source)`
- **OS-Specific Handling**:
  - **Linux/macOS**: Standard `ln -s` via `unix::fs::symlink()`
  - **Windows**: 
    - Uses `win_fs::symlink_dir()` for directories
    - Uses `win_fs::symlink_file()` for files
    - **Note**: Requires admin privileges OR Developer Mode enabled
    - Provides helpful error messages if symlink creation fails

#### 4. Semantic Deduplication
- **Struct**: `SemanticDeduplication`
- **Method**: `deduplicate_package(package_path, name, version)`
- **Process**:
  1. Generate canonical path in global store
  2. If canonical doesn't exist, hard-link package files to global store
  3. If package_path is not already a symlink, replace it with symlink to canonical
  4. Uses temporary paths for safe atomic replacement

### Usage Example

```rust
use crate::symlink::SemanticDeduplication;

let dedup = SemanticDeduplication::new()?;
dedup.deduplicate_package(
    &PathBuf::from("/project/node_modules/react"),
    "react",
    "18.2.0"
)?;
```

## Step 5: LRU/ML-Driven Caching

### Implementation Location
- **LRU Cache**: `core/src/cache.rs` (`PackageLruCache`)
- **ML Predictor**: `core/src/ml.rs` (`PredictiveOptimizer`)
- **Usage Tracker**: `core/src/usage_tracker.rs` (`UsageTracker`)
- **Integration**: `core/src/optimization.rs` (OptimizationEngine)

### LRU Cache Strategy

#### Data Structure
- **Type**: `PackageLruCache`
- **Key Format**: `PackageName@Version` (e.g., `react@18.2.0`)
- **Value**: `PackageUsageMetrics` containing:
  - `last_access_time`: From filesystem `atime`
  - `last_script_execution`: Timestamp of last script execution
  - `access_count`: Number of times accessed
  - `script_execution_count`: Number of script executions
  - `last_successful_build`: Timestamp of last successful build

#### Usage Metrics
- **Triggered by**:
  1. Storage Scanner observing `atime` of package files
  2. Successful script execution (`npm run build`, `npm test`)
  3. Successful build completion

#### Cleanup Strategy
- Packages at the "tail" of LRU queue (least recently used) are first candidates for cleanup
- `should_keep_lru()` checks if package was accessed within threshold days

### Predictive ML Model

#### Model Type
- **Current**: Rule-based classifier (extensible to actual ML models)
- **Target**: Binary classification (Keep vs. Purge)
- **Prediction Window**: 7 days (configurable)

#### Features (10 total)

1. **Days since last access** - `atime` from filesystem
2. **Days since last script execution** - From usage tracking
3. **Days since last successful build** - From build tracking
4. **Access frequency** - Normalized access count
5. **Script execution frequency** - Normalized script count
6. **Days since last commit** - Project activity indicator
7. **Project type score** - Framework-specific (React, TypeScript, etc.)
8. **Dependency count** - Normalized project complexity
9. **Days since last build** - From developer behavior
10. **File access frequency** - Developer activity level

#### Decision Rules

The `PredictiveOptimizer` uses a combination of:
1. **Simple rules** (e.g., keep if accessed in last 7 days)
2. **Weighted score** (logistic regression-like)
3. **Sigmoid function** to bound output between 0 and 1

#### Project Type Detection

Enhanced `detect_project_type()` function:
- Analyzes `package.json` for framework dependencies
- Checks for config files (`tsconfig.json`, `next.config.js`)
- Detects: React, Vue, Angular, TypeScript, Next.js, Node.js

### Integration: OptimizationEngine

The `OptimizationEngine` combines all strategies:

```rust
pub struct OptimizationEngine {
    deduplication: Option<SemanticDeduplication>,
    lru_cache: Option<PackageLruCache>,
    ml_predictor: Option<PredictiveOptimizer>,
    config: RulesConfig,
}
```

#### Optimization Process

1. **Scan**: Collect packages and projects
2. **Build Metrics**: Create usage metrics from scan data
3. **LRU Tracking**: Record access in LRU cache
4. **ML Prediction**: Check if package should be kept based on ML model
5. **LRU Check**: Check if package should be kept based on LRU strategy
6. **Decision**: Remove if orphaned OR (old AND not kept by ML AND not kept by LRU)
7. **Symlink**: Execute symlinking for duplicate packages

### Configuration

```rust
pub struct RulesConfig {
    pub preserve_days: i64,              // Days to preserve packages
    pub enable_symlinking: bool,        // Enable cross-project symlinking
    pub enable_ml_prediction: bool,     // Enable ML-based predictions
    pub lru_max_packages: usize,         // Max packages in LRU cache
    pub lru_max_size_bytes: u64,        // Max size of LRU cache
}
```

## Usage Example

```rust
use crate::optimization::{OptimizationEngine, RulesConfig};

// Configure
let config = RulesConfig {
    preserve_days: 90,
    enable_symlinking: true,
    enable_ml_prediction: true,
    lru_max_packages: 1000,
    lru_max_size_bytes: 10_000_000_000, // 10GB
};

// Create engine
let mut engine = OptimizationEngine::new(config)?;

// Scan and optimize
let scan = scanner::scan(&paths)?;
let report = engine.plan_optimized_cleanup(&scan)?;

// Execute symlinking
let symlinked_count = engine.execute_symlinking(&scan)?;

println!("Symlinked {} duplicate packages", symlinked_count);
println!("Estimated savings: {} bytes", report.total_estimated_bytes);
```

## Technical Details

### Windows Symlink Requirements

On Windows, creating directory symlinks requires:
- **Option 1**: Administrator privileges
- **Option 2**: Developer Mode enabled
  - Settings → Update & Security → For Developers → Developer Mode

The implementation provides helpful error messages if symlink creation fails.

### Hard Link Limitations

- **Windows**: Hard links only work on the same volume. Falls back to file copy if different volumes.
- **Unix**: Hard links work across the same filesystem.

### Performance Considerations

- **LRU Cache**: O(1) operations for get/put
- **ML Prediction**: O(1) per package (rule-based, extensible to ML models)
- **Symlinking**: O(n) where n = number of files in package directory
- **Hard Linking**: O(n) where n = number of files

## Future Enhancements

1. **Actual ML Models**: Replace rule-based classifier with trained models
2. **Script Execution Tracking**: Integrate with npm/yarn execution hooks
3. **Git Integration**: Extract last commit dates from git history
4. **Persistence**: Save/load usage metrics across runs
5. **Compression**: Add compression for packages at LRU tail
6. **Metrics Dashboard**: Visualize package usage patterns

## Testing

Basic tests are included in `symlink.rs`. To extend:

```rust
#[test]
fn test_symlink_deduplication() {
    // Test symlink creation and deduplication
}

#[test]
fn test_lru_cache_eviction() {
    // Test LRU eviction logic
}

#[test]
fn test_ml_prediction() {
    // Test ML prediction accuracy
}
```

