/**
 * Core utilities for PackagePurge - shared between CLI and bindings
 * Includes streaming support for progressive output
 */
import { spawn, ChildProcess } from 'child_process';
import * as path from 'path';
import * as fs from 'fs';
import { EventEmitter } from 'events';

// Cache the binary path
let cachedBinaryPath: string | null = null;

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
 * Get the path to the packagepurge-core binary (cached)
 * Search order:
 * 1. PACKAGEPURGE_CORE environment variable
 * 2. Local release build (core/target/release)
 * 3. Local debug build (core/target/debug)
 * 4. PATH
 */
export function coreBinary(): string {
	// Return cached path if available
	if (cachedBinaryPath && fileExists(cachedBinaryPath)) {
		return cachedBinaryPath;
	}

	// 1) Env override
	const envPath = process.env.PACKAGEPURGE_CORE;
	if (envPath && fileExists(envPath)) {
		cachedBinaryPath = envPath;
		return envPath;
	}

	// 2) Local release/debug
	const exe = isWindows() ? 'packagepurge_core.exe' : 'packagepurge-core';
	const rel = path.join(process.cwd(), 'core', 'target', 'release', exe);
	if (fileExists(rel)) {
		cachedBinaryPath = rel;
		return rel;
	}
	const dbg = path.join(process.cwd(), 'core', 'target', 'debug', exe);
	if (fileExists(dbg)) {
		cachedBinaryPath = dbg;
		return dbg;
	}

	// 3) PATH
	const fromPath = resolveFromPATH(isWindows() ? 'packagepurge_core' : 'packagepurge-core');
	if (fromPath) {
		cachedBinaryPath = fromPath;
		return fromPath;
	}

	throw new Error('packagepurge-core binary not found. Build with "npm run build:core" or set PACKAGEPURGE_CORE.');
}

/**
 * Invalidate cached binary path (for dev mode hot reload)
 */
export function invalidateBinaryCache(): void {
	cachedBinaryPath = null;
}

export interface CoreResult {
	stdout: string;
	stderr: string;
	code: number;
}

/**
 * Progress callback for streaming operations
 */
export interface StreamProgress {
	type: 'package' | 'project' | 'plan_item' | 'status' | 'raw';
	data: any;
	count: number;
}

/**
 * Streaming JSON parser for progressive output
 * Detects JSON objects/arrays in the stream and emits them as they complete
 */
class StreamingJsonParser extends EventEmitter {
	private buffer = '';
	private objectCount = 0;

	/**
	 * Feed data into the parser
	 */
	feed(chunk: string): void {
		this.buffer += chunk;
		this.tryParse();
	}

	/**
	 * Get any remaining unparsed content
	 */
	getRemaining(): string {
		return this.buffer;
	}

	private tryParse(): void {
		// Try to find complete JSON objects in the buffer
		// Look for package records as they stream
		const patterns = [
			{ regex: /"name"\s*:\s*"[^"]+"\s*,\s*"version"\s*:\s*"[^"]+"/g, type: 'package' },
			{ regex: /"path"\s*:\s*"[^"]+"\s*,\s*"manager"/g, type: 'project' },
			{ regex: /"target_path"\s*:\s*"[^"]+"/g, type: 'plan_item' },
		];

		for (const { regex, type } of patterns) {
			const matches = this.buffer.match(regex);
			if (matches) {
				for (const match of matches) {
					this.objectCount++;
					this.emit('progress', {
						type,
						data: match,
						count: this.objectCount
					} as StreamProgress);
				}
			}
		}
	}
}

/**
 * Run the packagepurge-core binary with streaming support
 */
export function runCoreStreaming(
	args: string[],
	onProgress?: (progress: StreamProgress) => void
): Promise<CoreResult> {
	return new Promise((resolve, reject) => {
		const bin = coreBinary();
		const child = spawn(bin, args, {
			stdio: ['ignore', 'pipe', 'pipe'],
			env: process.env
		});

		let out = '';
		let err = '';
		const parser = new StreamingJsonParser();

		// Wire up progress events
		if (onProgress) {
			parser.on('progress', onProgress);
		}

		child.stdout.on('data', (d) => {
			const chunk = d.toString();
			out += chunk;
			parser.feed(chunk);
		});

		child.stderr.on('data', (d) => {
			err += d.toString();
		});

		child.on('error', reject);
		child.on('close', (code) => {
			resolve({ stdout: out, stderr: err, code: code ?? 1 });
		});
	});
}

/**
 * Run the packagepurge-core binary with the given arguments (legacy non-streaming)
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

/**
 * Run core with timeout
 */
export function runCoreWithTimeout(
	args: string[],
	timeoutMs: number = 300000 // 5 minutes default
): Promise<CoreResult> {
	return new Promise((resolve, reject) => {
		const bin = coreBinary();
		const child = spawn(bin, args, {
			stdio: ['ignore', 'pipe', 'pipe'],
			env: process.env
		});

		let out = '';
		let err = '';
		let killed = false;

		const timeout = setTimeout(() => {
			killed = true;
			child.kill('SIGTERM');
			reject(new Error(`Command timed out after ${timeoutMs}ms`));
		}, timeoutMs);

		child.stdout.on('data', (d) => out += d.toString());
		child.stderr.on('data', (d) => err += d.toString());
		child.on('error', (e) => {
			clearTimeout(timeout);
			reject(e);
		});
		child.on('close', (code) => {
			clearTimeout(timeout);
			if (!killed) {
				resolve({ stdout: out, stderr: err, code: code ?? 1 });
			}
		});
	});
}
