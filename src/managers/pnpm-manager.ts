/**
 * pnpm package manager integration
 */
import { BasePackageManager } from './base-manager';
import { PackageManager, PackageInfo } from '../types';
import * as os from 'os';
import * as path from 'path';
import * as fs from 'fs-extra';

export class PnpmManager extends BasePackageManager {
  readonly manager = PackageManager.PNPM;
  readonly lockFileName = 'pnpm-lock.yaml';

  async getCachePath(): Promise<string> {
    // pnpm store path: ~/.pnpm-store on Unix, %LOCALAPPDATA%/pnpm/store on Windows
    if (process.platform === 'win32') {
      return path.join(os.homedir(), 'AppData', 'Local', 'pnpm', 'store');
    }
    return path.join(os.homedir(), '.pnpm-store');
  }

  async parseLockFile(lockFilePath: string): Promise<Map<string, string>> {
    const dependencies = new Map<string, string>();

    try {
      const lockFileContent = await fs.readFile(lockFilePath, 'utf-8');
      
      // Parse pnpm-lock.yaml format (YAML)
      const lines = lockFileContent.split('\n');
      let currentPackage: string | null = null;
      let currentVersion: string | null = null;

      for (const line of lines) {
        const trimmed = line.trim();
        
        // Package entry: "package-name:"
        if (trimmed.endsWith(':') && !trimmed.startsWith(' ') && !trimmed.startsWith('#')) {
          const match = trimmed.match(/^(.+?):$/);
          if (match && !match[1].startsWith('lockfile')) {
            currentPackage = match[1];
          }
        }
        
        // Version field
        if (trimmed.startsWith('version:') && currentPackage) {
          const match = trimmed.match(/version:\s*(.+?)$/);
          if (match) {
            currentVersion = match[1].replace(/["']/g, '');
            dependencies.set(currentPackage, currentVersion);
            currentPackage = null;
            currentVersion = null;
          }
        }
      }
    } catch (error) {
      // If lock file parsing fails, return empty map
    }

    return dependencies;
  }

  protected async analyzePackage(packagePath: string): Promise<PackageInfo | null> {
    try {
      const stat = await fs.stat(packagePath);
      const packageJsonPath = path.join(packagePath, 'package.json');
      
      if (!(await fs.pathExists(packageJsonPath))) {
        return null;
      }

      const packageJson = await fs.readJson(packageJsonPath);
      const size = await this.calculateDirectorySize(packagePath);

      return {
        name: packageJson.name || path.basename(packagePath),
        version: packageJson.version || 'unknown',
        path: packagePath,
        lastAccessed: stat.atime,
        size,
        manager: this.manager,
        projectPaths: [],
      };
    } catch {
      return null;
    }
  }

  private async calculateDirectorySize(dirPath: string): Promise<number> {
    let size = 0;
    try {
      const entries = await fs.readdir(dirPath);
      for (const entry of entries) {
        const entryPath = path.join(dirPath, entry);
        const stat = await fs.stat(entryPath);
        if (stat.isDirectory()) {
          size += await this.calculateDirectorySize(entryPath);
        } else {
          size += stat.size;
        }
      }
    } catch {
      // Ignore errors
    }
    return size;
  }
}

