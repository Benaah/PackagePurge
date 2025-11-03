/**
 * npm package manager integration
 */
import { BasePackageManager } from './base-manager';
import { PackageManager, PackageInfo } from '../types';
import * as os from 'os';
import * as path from 'path';
import * as fs from 'fs-extra';

export class NpmManager extends BasePackageManager {
  readonly manager = PackageManager.NPM;
  readonly lockFileName = 'package-lock.json';

  async getCachePath(): Promise<string> {
    // npm cache path: ~/.npm on Unix, %AppData%/npm-cache on Windows
    if (process.platform === 'win32') {
      return path.join(os.homedir(), 'AppData', 'Roaming', 'npm-cache');
    }
    return path.join(os.homedir(), '.npm');
  }

  async parseLockFile(lockFilePath: string): Promise<Map<string, string>> {
    const dependencies = new Map<string, string>();

    try {
      const lockFile = await fs.readJson(lockFilePath);
      
      // Parse package-lock.json structure
      function extractDependencies(obj: any, prefix: string = ''): void {
        if (obj.dependencies) {
          for (const [name, dep] of Object.entries(obj.dependencies as Record<string, any>)) {
            const fullName = prefix ? `${prefix}/${name}` : name;
            if (dep.version) {
              dependencies.set(fullName, dep.version);
            }
            if (dep.dependencies) {
              extractDependencies(dep, fullName);
            }
          }
        }
      }

      extractDependencies(lockFile);
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

