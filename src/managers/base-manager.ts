/**
 * Base class for package manager integrations
 */
import { PackageManager, PackageInfo, ProjectInfo } from '../types';
import * as fs from 'fs-extra';
import * as path from 'path';

export abstract class BasePackageManager {
  abstract readonly manager: PackageManager;
  abstract readonly lockFileName: string;

  /**
   * Get the cache directory path for this manager
   */
  abstract getCachePath(): Promise<string>;

  /**
   * Parse lock file to extract dependency information
   */
  abstract parseLockFile(lockFilePath: string): Promise<Map<string, string>>;

  /**
   * Scan for all packages in cache
   */
  async scanPackages(): Promise<PackageInfo[]> {
    const cachePath = await this.getCachePath();
    if (!(await fs.pathExists(cachePath))) {
      return [];
    }

    const packages: PackageInfo[] = [];
    const entries = await fs.readdir(cachePath);

    for (const entry of entries) {
      const entryPath = path.join(cachePath, entry);
      const stat = await fs.stat(entryPath);

      if (stat.isDirectory()) {
        const packageInfo = await this.analyzePackage(entryPath);
        if (packageInfo) {
          packages.push(packageInfo);
        }
      }
    }

    return packages;
  }

  /**
   * Analyze a single package directory
   */
  protected abstract analyzePackage(packagePath: string): Promise<PackageInfo | null>;

  /**
   * Find all projects using this manager
   */
  async findProjects(rootDir: string = process.cwd()): Promise<ProjectInfo[]> {
    const projects: ProjectInfo[] = [];
    const lockFilePattern = this.lockFileName;

    const searchDir = async (dir: string, depth: number = 0): Promise<void> => {
      if (depth > 10) return; // Limit depth to prevent infinite recursion

      try {
        const entries = await fs.readdir(dir, { withFileTypes: true });
        const entryNames = entries.map(e => e.name);
        const hasLockFile = entryNames.includes(lockFilePattern);
        const hasPackageJson = entryNames.includes('package.json');

        if (hasLockFile && hasPackageJson) {
          const packageJsonPath = path.join(dir, 'package.json');
          const lockFilePath = path.join(dir, lockFilePattern);
          const packageJson = await fs.readJson(packageJsonPath);
          const dependencies = await this.parseLockFile(lockFilePath);
          const stat = await fs.stat(packageJsonPath);

          projects.push({
            path: dir,
            manager: this.manager,
            dependencies,
            lastModified: stat.mtime,
            lockFilePath,
          });
        }

        // Recursively search subdirectories
        for (const entry of entries) {
          if (entry.isDirectory() && !entry.name.startsWith('.') && entry.name !== 'node_modules') {
             await searchDir(path.join(dir, entry.name), depth + 1);
          }
        }
      } catch (error) {
        // Skip directories we can't read
      }
    };

    await searchDir(rootDir);
    return projects;
  }

  /**
   * Get package version from directory name or package.json
   */
  protected async getPackageVersion(packagePath: string): Promise<string | null> {
    const packageJsonPath = path.join(packagePath, 'package.json');
    if (await fs.pathExists(packageJsonPath)) {
      try {
        const packageJson = await fs.readJson(packageJsonPath);
        return packageJson.version || null;
      } catch {
        return null;
      }
    }
    return null;
  }
}

