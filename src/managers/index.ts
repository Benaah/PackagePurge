/**
 * Package manager factory and exports
 */
import { BasePackageManager } from './base-manager';
import { NpmManager } from './npm-manager';
import { YarnManager } from './yarn-manager';
import { PnpmManager } from './pnpm-manager';
import { PackageManager } from '../types';

export { BasePackageManager, NpmManager, YarnManager, PnpmManager };

export function getManager(manager: PackageManager): BasePackageManager {
  switch (manager) {
    case PackageManager.NPM:
      return new NpmManager();
    case PackageManager.YARN:
      return new YarnManager();
    case PackageManager.PNPM:
      return new PnpmManager();
    default:
      throw new Error(`Unsupported package manager: ${manager}`);
  }
}

export function getAllManagers(): BasePackageManager[] {
  return [
    new NpmManager(),
    new YarnManager(),
    new PnpmManager(),
  ];
}

