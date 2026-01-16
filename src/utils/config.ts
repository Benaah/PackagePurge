/**
 * Configuration Loader for PackagePurge CLI
 * 
 * Supports loading configuration from:
 * - .packagepurgerc (JSON)
 * - .packagepurgerc.json
 * - .packagepurgerc.yaml / .packagepurgerc.yml
 * - packagepurge.config.js
 * - package.json "packagepurge" key
 * 
 * Configuration is merged with CLI arguments (CLI takes precedence)
 */

import * as fs from 'fs';
import * as path from 'path';
import * as yaml from 'yaml';

/**
 * PackagePurge configuration options
 */
export interface PackagePurgeConfig {
    /** Days to preserve packages (default: 90) */
    preserveDays?: number;
    /** Paths to scan (default: current directory) */
    paths?: string[];
    /** Paths/patterns to exclude from scanning */
    exclude?: string[];
    /** Enable symlinking for duplicate packages */
    enableSymlinking?: boolean;
    /** Enable ML-based predictions */
    enableMl?: boolean;
    /** Maximum packages in LRU cache */
    lruMaxPackages?: number;
    /** Maximum size of LRU cache in bytes */
    lruMaxSizeBytes?: number;
    /** Quarantine settings */
    quarantine?: {
        /** Maximum quarantine size in GB */
        maxSizeGb?: number;
        /** Days to retain quarantine entries */
        retentionDays?: number;
    };
    /** Output format preference */
    format?: 'table' | 'json' | 'yaml';
    /** Quiet mode */
    quiet?: boolean;
    /** Verbose mode */
    verbose?: boolean;
}

/**
 * Default configuration values
 */
export const DEFAULT_CONFIG: PackagePurgeConfig = {
    preserveDays: 90,
    paths: [],
    exclude: ['**/node_modules/.cache/**'],
    enableSymlinking: false,
    enableMl: false,
    lruMaxPackages: 1000,
    lruMaxSizeBytes: 10_000_000_000, // 10GB
    quarantine: {
        maxSizeGb: 10,
        retentionDays: 30,
    },
    format: 'table',
    quiet: false,
    verbose: false,
};

/**
 * Configuration file search locations (in order)
 */
const CONFIG_FILES = [
    '.packagepurgerc',
    '.packagepurgerc.json',
    '.packagepurgerc.yaml',
    '.packagepurgerc.yml',
    'packagepurge.config.js',
    'packagepurge.config.json',
];

/**
 * Find configuration file by walking up directory tree
 */
function findConfigFile(startDir: string): string | null {
    let dir = startDir;
    const root = path.parse(dir).root;

    while (dir !== root) {
        for (const filename of CONFIG_FILES) {
            const configPath = path.join(dir, filename);
            if (fs.existsSync(configPath)) {
                return configPath;
            }
        }
        dir = path.dirname(dir);
    }

    return null;
}

/**
 * Parse configuration from file content
 */
function parseConfigFile(filepath: string): Partial<PackagePurgeConfig> {
    const content = fs.readFileSync(filepath, 'utf-8');
    const ext = path.extname(filepath).toLowerCase();
    const basename = path.basename(filepath);

    // YAML files
    if (ext === '.yaml' || ext === '.yml') {
        return yaml.parse(content) as Partial<PackagePurgeConfig>;
    }

    // JS files
    if (ext === '.js') {
        // eslint-disable-next-line @typescript-eslint/no-var-requires
        const config = require(filepath);
        return config.default || config;
    }

    // JSON files (including .packagepurgerc without extension)
    try {
        return JSON.parse(content) as Partial<PackagePurgeConfig>;
    } catch {
        // If JSON parse fails, try YAML (for extensionless .packagepurgerc)
        return yaml.parse(content) as Partial<PackagePurgeConfig>;
    }
}

/**
 * Check for packagepurge key in package.json
 */
function loadFromPackageJson(startDir: string): Partial<PackagePurgeConfig> | null {
    let dir = startDir;
    const root = path.parse(dir).root;

    while (dir !== root) {
        const pkgPath = path.join(dir, 'package.json');
        if (fs.existsSync(pkgPath)) {
            try {
                const pkg = JSON.parse(fs.readFileSync(pkgPath, 'utf-8'));
                if (pkg.packagepurge) {
                    return pkg.packagepurge as Partial<PackagePurgeConfig>;
                }
            } catch {
                // Ignore parse errors
            }
        }
        dir = path.dirname(dir);
    }

    return null;
}

/**
 * Load and merge configuration from all sources
 */
export function loadConfig(cwd: string = process.cwd()): { config: PackagePurgeConfig; source: string | null } {
    // Start with defaults
    let config = { ...DEFAULT_CONFIG };
    let source: string | null = null;

    // 1. Try to find config file
    const configFile = findConfigFile(cwd);
    if (configFile) {
        try {
            const fileConfig = parseConfigFile(configFile);
            config = mergeConfig(config, fileConfig);
            source = configFile;
        } catch (error) {
            console.error(`Warning: Failed to parse config file ${configFile}:`, error);
        }
    }

    // 2. Check package.json if no config file found
    if (!source) {
        const pkgConfig = loadFromPackageJson(cwd);
        if (pkgConfig) {
            config = mergeConfig(config, pkgConfig);
            source = 'package.json';
        }
    }

    return { config, source };
}

/**
 * Deep merge configuration objects
 */
function mergeConfig(base: PackagePurgeConfig, override: Partial<PackagePurgeConfig>): PackagePurgeConfig {
    const result = { ...base };

    for (const [key, value] of Object.entries(override)) {
        if (value === undefined) continue;

        if (key === 'quarantine' && typeof value === 'object') {
            result.quarantine = {
                ...result.quarantine,
                ...value,
            };
        } else if (key === 'paths' && Array.isArray(value)) {
            result.paths = value;
        } else if (key === 'exclude' && Array.isArray(value)) {
            result.exclude = value;
        } else {
            (result as any)[key] = value;
        }
    }

    return result;
}

/**
 * Merge CLI options with loaded config (CLI takes precedence)
 */
export function mergeWithCliOptions(
    config: PackagePurgeConfig,
    cliOptions: Partial<PackagePurgeConfig>
): PackagePurgeConfig {
    return mergeConfig(config, cliOptions);
}

/**
 * Detect workspace configuration (pnpm/yarn/npm workspaces)
 */
export interface WorkspaceInfo {
    type: 'pnpm' | 'yarn' | 'npm' | 'lerna' | null;
    root: string;
    packages: string[];
}

export function detectWorkspace(startDir: string = process.cwd()): WorkspaceInfo {
    let dir = startDir;
    const root = path.parse(dir).root;

    while (dir !== root) {
        // pnpm workspace
        const pnpmWorkspace = path.join(dir, 'pnpm-workspace.yaml');
        if (fs.existsSync(pnpmWorkspace)) {
            try {
                const content = yaml.parse(fs.readFileSync(pnpmWorkspace, 'utf-8'));
                return {
                    type: 'pnpm',
                    root: dir,
                    packages: content.packages || [],
                };
            } catch { /* ignore */ }
        }

        // yarn/npm workspaces in package.json
        const pkgPath = path.join(dir, 'package.json');
        if (fs.existsSync(pkgPath)) {
            try {
                const pkg = JSON.parse(fs.readFileSync(pkgPath, 'utf-8'));
                if (pkg.workspaces) {
                    const workspaces = Array.isArray(pkg.workspaces)
                        ? pkg.workspaces
                        : pkg.workspaces.packages || [];

                    // Detect if yarn or npm based on lockfile
                    const hasYarnLock = fs.existsSync(path.join(dir, 'yarn.lock'));
                    return {
                        type: hasYarnLock ? 'yarn' : 'npm',
                        root: dir,
                        packages: workspaces,
                    };
                }
            } catch { /* ignore */ }
        }

        // Lerna
        const lernaPath = path.join(dir, 'lerna.json');
        if (fs.existsSync(lernaPath)) {
            try {
                const lerna = JSON.parse(fs.readFileSync(lernaPath, 'utf-8'));
                return {
                    type: 'lerna',
                    root: dir,
                    packages: lerna.packages || ['packages/*'],
                };
            } catch { /* ignore */ }
        }

        dir = path.dirname(dir);
    }

    return { type: null, root: startDir, packages: [] };
}

/**
 * Generate example configuration file content
 */
export function generateExampleConfig(): string {
    return `# PackagePurge Configuration
# Place this file as .packagepurgerc.yaml in your project root

# Days to preserve packages (default: 90)
preserveDays: 90

# Paths to scan (defaults to current directory)
paths:
  - .

# Patterns to exclude from scanning
exclude:
  - "**/node_modules/.cache/**"
  - "**/dist/**"

# Enable symlinking for duplicate packages
enableSymlinking: false

# Enable ML-based predictions
enableMl: false

# LRU cache settings
lruMaxPackages: 1000
lruMaxSizeBytes: 10000000000  # 10GB

# Quarantine settings
quarantine:
  maxSizeGb: 10
  retentionDays: 30

# Output format: table, json, yaml
format: table

# Quiet mode (minimal output)
quiet: false

# Verbose mode (detailed output)
verbose: false
`;
}
