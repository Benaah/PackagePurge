import { Command } from 'commander';
import { spawn } from 'child_process';
import * as path from 'path';
import * as fs from 'fs';
import { logger } from '../utils/logger';
import YAML from 'yaml';

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

function runCore(args: string[]): Promise<{ stdout: string; stderr: string; code: number }>
{
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

async function output(data: string, format: 'json' | 'yaml') {
	if (format === 'yaml') {
		try {
			const obj = JSON.parse(data);
			console.log(YAML.stringify(obj));
			return;
		} catch {
			// Fallback to raw
		}
	}
	console.log(data);
}

const program = new Command();
program
	.name('packagepurge')
	.description('Intelligent package manager cache cleanup service with project-aware optimization')
	.version('1.0.0')
	.option('-q, --quiet', 'Minimal output', false)
	.option('-v, --verbose', 'Verbose logging', false)
	.option('-f, --format <format>', 'Output format: json|yaml', 'json');

program.hook('preAction', (_, actionCommand) => {
	const opts = actionCommand.optsWithGlobals();
	if (opts.verbose) logger.setLevel(0);
});

program
	.command('scan')
	.description('Scan filesystem and output results')
	.option('-p, --paths <paths...>', 'Paths to scan', [])
	.action(async (opts, cmd) => {
		const g = cmd.parent?.opts?.() || {};
		const args = ['scan', ...(opts.paths?.length ? ['--paths', ...opts.paths] : [])];
		const res = await runCore(args);
		if (res.code !== 0) {
			if (!g.quiet) logger.error(res.stderr || 'Scan failed');
			process.exit(res.code);
		}
		await output(res.stdout, (g.format || 'json'));
	});

program
	.command('analyze')
	.description('Dry-run cleanup plan (no changes)')
	.option('-p, --paths <paths...>', 'Paths to analyze', [])
	.option('-d, --preserve-days <days>', 'Preserve days for recency', '90')
	.action(async (opts, cmd) => {
		const g = cmd.parent?.opts?.() || {};
		const preserve = String(opts.preserveDays ?? opts['preserve-days'] ?? opts.d ?? 90);
		const args = ['dry-run', '--preserve-days', preserve, ...(opts.paths?.length ? ['--paths', ...opts.paths] : [])];
		const res = await runCore(args);
		if (res.code !== 0) {
			if (!g.quiet) logger.error(res.stderr || 'Analyze failed');
			process.exit(res.code);
		}
		await output(res.stdout, (g.format || 'json'));
	});

program
	.command('clean')
	.description('Quarantine targets (Move-and-Delete transaction). Defaults to dry-run via analyze.')
	.option('-t, --targets <targets...>', 'Paths to quarantine (from analyze)')
	.action(async (opts, cmd) => {
		const g = cmd.parent?.opts?.() || {};
		if (!opts.targets || !opts.targets.length) {
			if (!g.quiet) logger.warn('No targets provided. Run analyze first to produce a plan.');
			process.exit(2);
		}
		const res = await runCore(['quarantine', ...opts.targets]);
		if (res.code !== 0) {
			if (!g.quiet) logger.error(res.stderr || 'Clean failed');
			process.exit(res.code);
		}
		await output(res.stdout, (g.format || 'json'));
	});

program
	.command('rollback')
	.description('Rollback quarantine by id or latest')
	.option('--id <id>', 'Quarantine record id')
	.option('--latest', 'Rollback the most recent quarantine', false)
	.action(async (opts, cmd) => {
		const g = cmd.parent?.opts?.() || {};
		const args = ['rollback'];
		const finalArgs = opts.id ? args.concat(['--id', opts.id]) : (opts.latest ? args.concat(['--latest']) : args);
		const res = await runCore(finalArgs);
		if (res.code !== 0) {
			if (!g.quiet) logger.error(res.stderr || 'Rollback failed');
			process.exit(res.code);
		}
		await output(res.stdout, (g.format || 'json'));
	});

program.parse(process.argv);
