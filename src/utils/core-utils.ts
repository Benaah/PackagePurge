/**
 * Core utilities for PackagePurge - shared between CLI and bindings
 */
import { spawn } from 'child_process';
import * as path from 'path';
import * as fs from 'fs';

/**
 * Check if running on Windows
 */
export function isWindows(): boolean {
	return process.platform === 'win32';
}

/**
 * Check if a file exists at the given path
 */
export function fileExists(p: string): boolean {
	try {
		return fs.existsSync(p);
	} catch {
		return false;
	}
}

/**
 * Resolve an executable from PATH environment variable
 */
export function resolveFromPATH(name: string): string | null {
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

/**
 * Get the path to the packagepurge-core binary
 * Search order:
 * 1. PACKAGEPURGE_CORE environment variable
 * 2. Local release build (core/target/release)
 * 3. Local debug build (core/target/debug)
 * 4. PATH
 */
export function coreBinary(): string {
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

export interface CoreResult {
	stdout: string;
	stderr: string;
	code: number;
}

/**
 * Run the packagepurge-core binary with the given arguments
 */
export function runCore(args: string[]): Promise<CoreResult> {
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
