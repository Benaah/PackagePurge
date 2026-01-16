# PackagePurge 🧹

**Intelligent package manager cache cleanup service with project-aware optimization**

[![npm version](https://badge.fury.io/js/packagepurge.svg)](https://www.npmjs.com/package/packagepurge)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

PackagePurge is a comprehensive service that solves the persistent "npm hell" storage problem by intelligently managing disk space across npm, yarn, and pnpm caches while maintaining safety and project awareness.

## 🎯 Features

- **Human-Readable Output**: Beautiful table format with color-coded status (JSON/YAML also available)
- **Intelligent LRU Caching**: Size-aware preservation of frequently used packages
- **Project-Aware Optimization**: Analyzes actual project dependencies before cleanup
- **Multi-Manager Support**: Works with npm, yarn, and pnpm
- **Safety-First Design**: Built-in backup and rollback mechanisms
- **Cross-Project Deduplication**: Reduces storage by symlinking common dependencies
- **ML-Powered Predictions**: Rule-based classifier for smart eviction decisions

## 📦 Installation

### Option 1: npm (Recommended)

```bash
npm install -g packagepurge
```

After installation, you can use either `purge` (recommended) or `packagepurge`:

```bash
purge --help
```

### Option 2: Download Binary

Download pre-built binaries from [GitHub Releases](https://github.com/Benaah/PackagePurge/releases):

| Platform | Binary |
|----------|--------|
| Windows x64 | `purge-windows-x64.exe` |
| Linux x64 | `purge-linux-x64` |
| macOS x64 | `purge-macos-x64` |
| macOS ARM64 (M1/M2) | `purge-macos-arm64` |

**Linux/macOS:**

```bash
# Download and make executable
chmod +x purge-linux-x64
sudo mv purge-linux-x64 /usr/local/bin/purge
purge --help
```

**Windows:**

```powershell
# Add to PATH or run directly
.\purge-windows-x64.exe --help
```

## 🚀 Quick Start

```bash
# Scan your projects for packages
purge scan --paths ./my-project

# Analyze cleanup opportunities (dry run)
purge analyze --preserve-days 30

# See what can be optimized with ML
purge optimize --enable-ml
```

## 📖 Usage

### Scan Filesystem

Scan directories to find packages and projects:

```bash
# Scan current directory
purge scan

# Scan specific paths
purge scan --paths ./project1 ./project2

# Output as JSON for scripting
purge scan --format json
```

**Sample Output:**

```
📦 Packages Found

Package                        Version      Size       Path
────────────────────────────────────────────────────────────────────────────────────────────────────
react                          18.2.0       125 KB     .../node_modules/react
lodash                         4.17.21      528 KB     .../node_modules/lodash
typescript                     5.3.3        65.2 MB    .../node_modules/typescript

📊 Total: 245 packages, 156.7 MB

📁 Projects Found: 3
   • C:\projects\my-app
   • C:\projects\api-server
```

### Analyze (Dry Run)

Preview what would be cleaned without making changes:

```bash
# Analyze with default 90-day preservation
purge analyze

# Custom preservation period
purge analyze --preserve-days 30 --paths ./my-project
```

**Sample Output:**

```
🧹 Cleanup Plan

Orphaned (12 packages, 45.2 MB)
────────────────────────────────────────────────────────────────────────────────
  • old-package                      12.5 MB     .../node_modules/old-package
  • unused-lib                       8.3 MB      .../node_modules/unused-lib

Outdated (8 packages, 23.1 MB)
────────────────────────────────────────────────────────────────────────────────
  • stale-dependency                 5.2 MB      .../cache/stale-dep

📊 Summary
   Total packages: 20
   Estimated savings: 68.3 MB
```

### Optimize with ML/LRU

Get intelligent cleanup recommendations:

```bash
# Basic optimization
purge optimize

# With ML predictions
purge optimize --enable-ml --preserve-days 90

# With symlinking for duplicates
purge optimize --enable-symlinking

# Full optimization
purge optimize --enable-ml --enable-symlinking --lru-max-packages 2000
```

### Execute Symlinking

Deduplicate packages across projects by creating symlinks:

```bash
purge symlink --paths ./project1 ./project2
```

> **Note (Windows)**: Symlinking requires Administrator privileges or Developer Mode enabled.

### Clean (Quarantine)

Move packages to quarantine (recoverable):

```bash
# First analyze to get targets
purge analyze --format json > plan.json

# Then quarantine targets
purge clean --targets /path/to/package1 /path/to/package2
```

### Rollback

Restore quarantined packages:

```bash
# Rollback latest quarantine
purge rollback --latest

# Rollback by ID
purge rollback --id <quarantine-id>
```

## 🎨 Output Formats

PackagePurge supports three output formats:

| Format | Flag | Description |
|--------|------|-------------|
| Table | `--format table` | Human-readable with colors (default) |
| JSON | `--format json` | Machine-readable for scripting |
| YAML | `--format yaml` | Alternative structured format |

```bash
# Default table output
purge scan

# JSON for piping to jq
purge scan --format json | jq '.packages | length'

# YAML output
purge analyze --format yaml
```

## 🔧 Configuration

PackagePurge supports configuration files in your project root. Create one with:

```bash
purge init
```

This creates `.packagepurgerc.yaml`:

```yaml
# Days to preserve packages (default: 90)
preserveDays: 90

# Paths to scan
paths:
  - .

# Patterns to exclude
exclude:
  - "**/node_modules/.cache/**"

# Enable symlinking for duplicates
enableSymlinking: false

# Enable ML-based predictions
enableMl: false

# Quarantine settings
quarantine:
  maxSizeGb: 10
  retentionDays: 30
```

### Configuration Commands

```bash
# View current configuration
purge config

# View config as JSON
purge config --json

# Show statistics (quarantine, cache, features)
purge stats

# Cleanup old quarantine entries
purge cleanup-quarantine --retention-days 30
```

### Workspace Detection

PackagePurge auto-detects monorepo workspaces:

- **pnpm**: `pnpm-workspace.yaml`
- **yarn/npm**: `package.json` workspaces field
- **lerna**: `lerna.json`

## 🏗️ Architecture

### Technical Stack

- **Core Engine**: Rust (high-performance scanning and optimization)
- **CLI**: TypeScript (Node.js) with Commander.js
- **LRU Cache**: Size-aware with ML-driven predictions
- **Symlinking**: Cross-platform hard linking and symbolic links

### How It Works

1. **Scan**: Discovers all `node_modules` and cache directories
2. **Analyze**: Builds dependency graph and identifies orphaned/stale packages
3. **Optimize**: Uses LRU cache + optional ML to predict which packages to keep
4. **Clean**: Moves targets to quarantine (reversible) or creates symlinks

## 📊 Cleanup Reasons

| Reason | Color | Description |
|--------|-------|-------------|
| Orphaned | Yellow | Not used by any project |
| Outdated | Gray | Not accessed within preserve period |
| ML: Unused | Magenta | ML predicts won't be needed |
| Size Pressure | Red | Cache at capacity, evicting largest |
| Symlink Candidate | Blue | Duplicate that can be deduplicated |

## 🛡️ Safety Features

- **Automatic backup** before cleanup
- **Rollback capability** for immediate restoration
- **Dry-run mode** for previewing changes
- **Project validation** before deletion
- **Quarantine system** - nothing is permanently deleted immediately

## 🔧 Troubleshooting

### Windows Symlink Issues

If symlinking fails on Windows:

1. **Enable Developer Mode**: Settings → For Developers → Developer Mode: On
2. **Or run as Administrator**

### Large Scan Times

For very large directories:

```bash
# Limit scan depth by specifying paths
purge scan --paths ./specific-project

# Use quiet mode to reduce output
purge scan -q --format json
```

## 🤝 Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## 📝 License

MIT © [Eng. Onyango Benard](https://github.com/Benaah)

## 🔗 Links

- [GitHub Repository](https://github.com/Benaah/PackagePurge)
- [npm Package](https://www.npmjs.com/package/packagepurge)
- [Report Issues](https://github.com/Benaah/PackagePurge/issues)
