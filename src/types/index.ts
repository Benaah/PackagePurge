/**
 * Core type definitions for PackagePurge service
 */

export enum PackageManager {
  NPM = 'npm',
  YARN = 'yarn',
  PNPM = 'pnpm',
}

export interface PackageInfo {
  name: string;
  version: string;
  path: string;
  lastAccessed: Date;
  size: number; // in bytes
  manager: PackageManager;
  projectPaths: string[]; // projects that use this package
}

export interface ProjectInfo {
  path: string;
  manager: PackageManager;
  dependencies: Map<string, string>; // package -> version
  lastModified: Date;
  lockFilePath?: string;
}

export interface CleanupStrategy {
  name: string;
  rules: CleanupRule[];
  mlEnabled: boolean;
}

export interface CleanupRule {
  name: string;
  condition: (packageInfo: PackageInfo) => boolean;
  priority: number;
}

export interface CleanupResult {
  packagesDeleted: PackageInfo[];
  spaceSaved: number; // in bytes
  rollbackId?: string;
  timestamp: Date;
}

export interface BackupInfo {
  id: string;
  timestamp: Date;
  packages: PackageInfo[];
  totalSize: number;
  archivePath: string;
}

export interface Analytics {
  totalSpaceSaved: number; // in bytes
  totalRollbacks: number;
  totalReinstalls: number;
  savingsToRiskRatio: number;
  cacheHitRate: number;
  projectsAnalyzed: number;
  lastCleanup: Date | null;
}

export interface OptimizationConfig {
  preserveDays: number;
  keepVersions: number;
  enableML: boolean;
  enableSymlinking: boolean;
  backupEnabled: boolean;
  managers: PackageManager[];
  dryRun: boolean;
  lruMaxPackages?: number;
  lruMaxSizeBytes?: number;
}

export interface OptimizeResult {
  items: Array<{
    target_path: string;
    estimated_size_bytes: number;
    reason: string;
  }>;
  total_estimated_bytes: number;
}

export interface SymlinkResult {
  status: string;
  symlinked_count: number;
}

export interface DependencyGraph {
  nodes: Map<string, PackageInfo>;
  edges: Map<string, string[]>; // package -> dependencies
  rootProjects: ProjectInfo[];
}

