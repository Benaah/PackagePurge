---
description: Build PackagePurge (Rust core + TypeScript CLI)
---

# Build PackagePurge

This workflow builds both the Rust core binary and the TypeScript CLI.

## Prerequisites

- Node.js >= 16.0.0
- Rust toolchain (cargo)
- npm

## Steps

// turbo-all

1. Install npm dependencies:
```bash
cd c:\Users\barne\OneDrive\Desktop\PackagePurge
npm install
```

2. Build Rust core (release mode):
```bash
cd c:\Users\barne\OneDrive\Desktop\PackagePurge\core
cargo build --release
```

3. Build TypeScript:
```bash
cd c:\Users\barne\OneDrive\Desktop\PackagePurge
npx tsc
```

4. Verify CLI works:
```bash
cd c:\Users\barne\OneDrive\Desktop\PackagePurge
node dist/cli/index.js --help
```

## Output

After successful build:
- Rust binary: `core/target/release/packagepurge_core.exe` (Windows) or `core/target/release/packagepurge-core` (Unix)
- TypeScript output: `dist/` directory
- CLI entry point: `dist/cli/index.js`

## Quick Build Command

For a full rebuild:
```bash
cd c:\Users\barne\OneDrive\Desktop\PackagePurge
npm run build
```

This runs both `build:core` (cargo) and `tsc` sequentially.
