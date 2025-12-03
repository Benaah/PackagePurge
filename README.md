# PackagePurge 🧹

**Intelligent package manager cache cleanup service with project-aware optimization**

PackagePurge is a comprehensive service that solves the persistent "npm hell" storage problem by intelligently managing disk space across npm, yarn, and pnpm caches while maintaining safety and project awareness.

## 🎯 Core Value Proposition

- **Intelligent Caching**: LRU-based preservation of frequently used packages
- **Project-Aware Optimization**: Analyzes actual project dependencies before cleanup
- **Multi-Manager Support**: Works with npm, yarn, and pnpm
- **Safety-First Design**: Built-in backup and rollback mechanisms
- **Cross-Project Deduplication**: Reduces storage by sharing common dependencies

## 🏗️ Architecture

### Core Components

1. **Storage Scanner**: Filesystem analysis with AST/dependency graph parsing
2. **Optimization Engine**: Rule-based and ML-driven cleanup strategies
3. **Safety Layer**: Backup and rollback functionality
4. **Reporting Dashboard**: Analytics with savings-to-risk ratio metrics

### Technical Stack

- **Core**: Rust (high-performance scanning and optimization)
- **CLI**: TypeScript (Node.js) with Commander.js
- **File Operations**: Rust stdlib + fs-extra for robust filesystem operations
- **Dependency Analysis**: Custom parsers for package.json, lock files
- **ML Component**: Rule-based classifier (extensible to actual ML models)
- **Symlinking**: Cross-platform hard linking and symbolic links
- **Caching**: LRU cache with ML-driven predictions

## 📦 Installation

```bash
npm install -g packagepurge
```

## 🚀 Usage

### Scan Filesystem
```bash
packagepurge scan
packagepurge scan --paths ./project1 ./project2
```

### Analyze (Dry Run)
```bash
packagepurge analyze
packagepurge analyze --paths ./my-project --preserve-days 90
```

### Optimize with ML/LRU and Symlinking
```bash
# Basic optimization
packagepurge optimize

# With ML predictions enabled
packagepurge optimize --enable-ml --preserve-days 90

# With symlinking enabled
packagepurge optimize --enable-symlinking

# Full optimization with all features
packagepurge optimize --enable-ml --enable-symlinking --lru-max-packages 2000
```

### Execute Symlinking
```bash
# Symlink duplicate packages across projects
packagepurge symlink --paths ./project1 ./project2
```

### Clean (Quarantine)
```bash
# First analyze to get targets
packagepurge analyze > plan.json

# Then quarantine targets
packagepurge clean --targets $(jq -r '.items[].target_path' plan.json)
```

### Rollback
```bash
# Rollback latest quarantine
packagepurge rollback --latest

# Rollback by ID
packagepurge rollback --id <quarantine-id>
```

## 🔧 Configuration

Create a `.packagepurge.json` file in your project root:

```json
{
  "preserveDays": 90,
  "keepVersions": 2,
  "enableML": false,
  "backupEnabled": true,
  "managers": ["npm", "yarn", "pnpm"]
}
```

## 📊 Metrics

PackagePurge tracks:
- **Disk Space Saved**: Total GB recovered
- **Savings-to-Risk Ratio**: `Disk Space Saved / (Rollbacks + Re-installs)`
- **Cache Hit Rate**: Percentage of packages preserved by LRU
- **Project Coverage**: Percentage of projects analyzed

## 🛡️ Safety Features

- Automatic backup creation before cleanup
- Rollback capability for immediate restoration
- Dry-run mode for previewing changes
- Project dependency validation before deletion

## 📝 License

MIT

