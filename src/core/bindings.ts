/**
 * TypeScript bindings for Rust core functionality
 * Provides type-safe interfaces to the Rust binary
 */

import { spawn } from 'child_process';
import * as path from 'path';
import * as fs from 'fs';
import { OptimizeResult, SymlinkResult } from '../types';

function isWindows(): boolean {
	return process.platform === 'win32';
}

function fileExists(p: string): boolean {
	try { return fs.existsSync(p); } catch { return false; }
}

function resolveFromPATH(name: string): string | null {
	const exts = isWindows() ? ['.exe', '.cmd', ''] : [''];
	const parts = (process.env.PATH || '').split(path.delimiter);
	for (const dir of parts) {
		for (const ext of exts) {
			const candidate = path.join(dir, name + ext);
			if (fileExists(candidate)) return candidate;
		}
	}
	return null;
}

function coreBinary(): string {
	// 1) Env override
	const envPath = process.env.PACKAGEPURGE_CORE;
	if (envPath && fileExists(envPath)) return envPath;
	// 2) Local release/debug
	const exe = isWindows() ? 'packagepurge_core.exe' : 'packagepurge-core';
	const rel = path.join(process.cwd(), 'core', 'target', 'release', exe);
	if (fileExists(rel)) return rel;
	const dbg = path.join(process.cwd(), 'core', 'target', 'debug', exe);
	if (fileExists(dbg)) return dbg;
	// 3) PATH
	const fromPath = resolveFromPATH(isWindows() ? 'packagepurge_core' : 'packagepurge-core');
	if (fromPath) return fromPath;
	throw new Error('packagepurge-core binary not found. Build with "npm run build:core" or set PACKAGEPURGE_CORE.');
}

function runCore(args: string[]): Promise<{ stdout: string; stderr: string; code: number }> {
	return new Promise((resolve, reject) => {
		const bin = coreBinary();
		const child = spawn(bin, args, { stdio: ['ignore', 'pipe', 'pipe'], env: process.env });
		let out = '';
		let err = '';
		child.stdout.on('data', (d) => out += d.toString());
		child.stderr.on('data', (d) => err += d.toString());
		child.on('error', reject);
		child.on('close', (code) => resolve({ stdout: out, stderr: err, code: code ?? 1 }));
	});
}

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

