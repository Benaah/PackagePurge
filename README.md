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

- **Language**: TypeScript (Node.js)
- **CLI**: Commander.js for command-line interface
- **File Operations**: fs-extra for robust filesystem operations
- **Dependency Analysis**: Custom parsers for package.json, lock files
- **ML Component**: Rule-based initially, extensible for ML models

## 📦 Installation

```bash
npm install -g packagepurge
```

## 🚀 Usage

### Basic Cleanup
```bash
packagepurge clean
```

### Project-Aware Analysis
```bash
packagepurge analyze --project ./my-project
```

### Custom Strategy
```bash
packagepurge clean --strategy aggressive --backup
```

### View Dashboard
```bash
packagepurge dashboard
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

