/**
 * TypeScript bindings for Rust core functionality
 * Provides type-safe interfaces to the Rust binary
 */

import { runCore } from '../utils/core-utils';
import { OptimizeResult, SymlinkResult } from '../types';

export interface OptimizeOptions {
	paths?: string[];
	preserveDays?: number;
	enableSymlinking?: boolean;
	enableML?: boolean;
	lruMaxPackages?: number;
	lruMaxSizeBytes?: number;
}

export interface SymlinkOptions {
	paths?: string[];
}

/**
 * Optimize packages with ML/LRU prediction and symlinking
 */
export async function optimize(options: OptimizeOptions = {}): Promise<OptimizeResult> {
	const args = ['optimize'];

	if (options.preserveDays !== undefined) {
		args.push('--preserve-days', String(options.preserveDays));
	}
	if (options.enableSymlinking) {
		args.push('--enable-symlinking');
	}
	if (options.enableML) {
		args.push('--enable-ml');
	}
	if (options.lruMaxPackages !== undefined) {
		args.push('--lru-max-packages', String(options.lruMaxPackages));
	}
	if (options.lruMaxSizeBytes !== undefined) {
		args.push('--lru-max-size-bytes', String(options.lruMaxSizeBytes));
	}
	if (options.paths && options.paths.length > 0) {
		args.push('--paths', ...options.paths);
	}

	const res = await runCore(args);
	if (res.code !== 0) {
		throw new Error(`Optimize failed: ${res.stderr || 'Unknown error'}`);
	}

	return JSON.parse(res.stdout) as OptimizeResult;
}

/**
 * Execute symlinking for duplicate packages
 */
export async function executeSymlinking(options: SymlinkOptions = {}): Promise<SymlinkResult> {
	const args = ['symlink'];

	if (options.paths && options.paths.length > 0) {
		args.push('--paths', ...options.paths);
	}

	const res = await runCore(args);
	if (res.code !== 0) {
		throw new Error(`Symlink failed: ${res.stderr || 'Unknown error'}`);
	}

	return JSON.parse(res.stdout) as SymlinkResult;
}

/**
 * Scan filesystem for packages and projects
 */
export async function scan(paths: string[] = []): Promise<any> {
	const args = ['scan'];
	if (paths.length > 0) {
		args.push('--paths', ...paths);
	}

	const res = await runCore(args);
	if (res.code !== 0) {
		throw new Error(`Scan failed: ${res.stderr || 'Unknown error'}`);
	}

	return JSON.parse(res.stdout);
}

/**
 * Analyze (dry run) cleanup plan
 */
export async function analyze(paths: string[] = [], preserveDays: number = 90): Promise<any> {
	const args = ['dry-run', '--preserve-days', String(preserveDays)];
	if (paths.length > 0) {
		args.push('--paths', ...paths);
	}

	const res = await runCore(args);
	if (res.code !== 0) {
		throw new Error(`Analyze failed: ${res.stderr || 'Unknown error'}`);
	}

	return JSON.parse(res.stdout);
}

/**
 * Quarantine packages (move to quarantine directory)
 */
export async function quarantine(targets: string[]): Promise<any> {
	if (!targets.length) {
		throw new Error('No targets provided for quarantine');
	}

	const res = await runCore(['quarantine', ...targets]);
	if (res.code !== 0) {
		throw new Error(`Quarantine failed: ${res.stderr || 'Unknown error'}`);
	}

	return JSON.parse(res.stdout);
}

/**
 * Rollback a quarantine operation
 */
export async function rollback(options: { id?: string; latest?: boolean } = {}): Promise<any> {
	const args = ['rollback'];
	if (options.id) {
		args.push('--id', options.id);
	} else if (options.latest) {
		args.push('--latest');
	} else {
		throw new Error('Either id or latest must be specified for rollback');
	}

	const res = await runCore(args);
	if (res.code !== 0) {
		throw new Error(`Rollback failed: ${res.stderr || 'Unknown error'}`);
	}

	return JSON.parse(res.stdout);
}
