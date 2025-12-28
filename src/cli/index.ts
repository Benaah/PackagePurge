#!/usr/bin/env node

import { Command } from 'commander';
import chalk from 'chalk';
import { logger } from '../utils/logger';
import { runCore } from '../utils/core-utils';
import { output, OutputFormat } from '../utils/formatter';

// Simple spinner for progress indication
class Spinner {
	private frames = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
	private current = 0;
	private interval: NodeJS.Timeout | null = null;
	private text: string;

	constructor(text: string) {
		this.text = text;
	}

	start(): void {
		process.stderr.write('\x1B[?25l'); // Hide cursor
		this.interval = setInterval(() => {
			process.stderr.write(`\r${chalk.cyan(this.frames[this.current])} ${this.text}`);
			this.current = (this.current + 1) % this.frames.length;
		}, 80);
	}

	update(text: string): void {
		this.text = text;
	}

	succeed(text?: string): void {
		this.stop();
		console.error(`\r${chalk.green('✓')} ${text || this.text}`);
	}

	fail(text?: string): void {
		this.stop();
		console.error(`\r${chalk.red('✗')} ${text || this.text}`);
	}

	private stop(): void {
		if (this.interval) {
			clearInterval(this.interval);
			this.interval = null;
		}
		process.stderr.write('\x1B[?25h'); // Show cursor
		process.stderr.write('\r\x1B[K'); // Clear line
	}
}

const program = new Command();
program
	.name('purge')
	.description('Intelligent package manager cache cleanup service with project-aware optimization')
	.version('1.0.1')
	.option('-q, --quiet', 'Minimal output', false)
	.option('-v, --verbose', 'Verbose logging', false)
	.option('-f, --format <format>', 'Output format: table|json|yaml', 'table');

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
		const format = (g.format || 'table') as OutputFormat;
		const spinner = !g.quiet && format === 'table' ? new Spinner('Scanning for packages...') : null;

		spinner?.start();

		const args = ['scan', ...(opts.paths?.length ? ['--paths', ...opts.paths] : [])];
		const res = await runCore(args);

		if (res.code !== 0) {
			spinner?.fail('Scan failed');
			if (!g.quiet) logger.error(res.stderr || 'Scan failed');
			process.exit(res.code);
		}

		spinner?.succeed('Scan complete');
		output(res.stdout, format, 'scan');
	});

program
	.command('analyze')
	.description('Dry-run cleanup plan (no changes)')
	.option('-p, --paths <paths...>', 'Paths to analyze', [])
	.option('-d, --preserve-days <days>', 'Preserve days for recency', '90')
	.action(async (opts, cmd) => {
		const g = cmd.parent?.opts?.() || {};
		const format = (g.format || 'table') as OutputFormat;
		const spinner = !g.quiet && format === 'table' ? new Spinner('Analyzing packages...') : null;

		spinner?.start();

		const preserve = String(opts.preserveDays ?? 90);
		const args = ['dry-run', '--preserve-days', preserve, ...(opts.paths?.length ? ['--paths', ...opts.paths] : [])];
		const res = await runCore(args);

		if (res.code !== 0) {
			spinner?.fail('Analysis failed');
			if (!g.quiet) logger.error(res.stderr || 'Analyze failed');
			process.exit(res.code);
		}

		spinner?.succeed('Analysis complete');
		output(res.stdout, format, 'analyze');
	});

program
	.command('clean')
	.description('Quarantine targets (Move-and-Delete transaction). Defaults to dry-run via analyze.')
	.option('-t, --targets <targets...>', 'Paths to quarantine (from analyze)')
	.action(async (opts, cmd) => {
		const g = cmd.parent?.opts?.() || {};
		const format = (g.format || 'table') as OutputFormat;

		if (!opts.targets || !opts.targets.length) {
			if (!g.quiet) {
				console.log(chalk.yellow('⚠ No targets provided.'));
				console.log(chalk.gray('  Run `packagepurge analyze` first to produce a cleanup plan.'));
				console.log(chalk.gray('  Then use: packagepurge clean --targets <path1> <path2> ...'));
			}
			process.exit(2);
		}

		const spinner = !g.quiet && format === 'table' ? new Spinner(`Quarantining ${opts.targets.length} packages...`) : null;
		spinner?.start();

		const res = await runCore(['quarantine', ...opts.targets]);

		if (res.code !== 0) {
			spinner?.fail('Quarantine failed');
			if (!g.quiet) logger.error(res.stderr || 'Clean failed');
			process.exit(res.code);
		}

		spinner?.succeed('Quarantine complete');
		output(res.stdout, format, 'quarantine');
	});

program
	.command('rollback')
	.description('Rollback quarantine by id or latest')
	.option('--id <id>', 'Quarantine record id')
	.option('--latest', 'Rollback the most recent quarantine', false)
	.action(async (opts, cmd) => {
		const g = cmd.parent?.opts?.() || {};
		const format = (g.format || 'table') as OutputFormat;

		if (!opts.id && !opts.latest) {
			if (!g.quiet) {
				console.log(chalk.yellow('⚠ No rollback target specified.'));
				console.log(chalk.gray('  Use: packagepurge rollback --latest'));
				console.log(chalk.gray('  Or:  packagepurge rollback --id <quarantine-id>'));
			}
			process.exit(2);
		}

		const spinner = !g.quiet && format === 'table' ? new Spinner('Rolling back...') : null;
		spinner?.start();

		const args = ['rollback'];
		if (opts.id) args.push('--id', opts.id);
		if (opts.latest) args.push('--latest');

		const res = await runCore(args);

		if (res.code !== 0) {
			spinner?.fail('Rollback failed');
			if (!g.quiet) logger.error(res.stderr || 'Rollback failed');
			process.exit(res.code);
		}

		spinner?.succeed('Rollback complete');
		output(res.stdout, format, 'rollback');
	});

program
	.command('optimize')
	.description('Optimize with ML/LRU prediction and symlinking (dry run)')
	.option('-p, --paths <paths...>', 'Paths to optimize', [])
	.option('-d, --preserve-days <days>', 'Days to preserve packages', '90')
	.option('--enable-symlinking', 'Enable cross-project symlinking', false)
	.option('--enable-ml', 'Enable ML-based predictions', false)
	.option('--lru-max-packages <count>', 'Maximum packages in LRU cache', '1000')
	.option('--lru-max-size-bytes <bytes>', 'Maximum size of LRU cache in bytes', '10000000000')
	.action(async (opts, cmd) => {
		const g = cmd.parent?.opts?.() || {};
		const format = (g.format || 'table') as OutputFormat;

		const features: string[] = [];
		if (opts.enableSymlinking) features.push('symlinking');
		if (opts.enableMl) features.push('ML');
		const featureStr = features.length ? ` (${features.join(', ')})` : '';

		const spinner = !g.quiet && format === 'table' ? new Spinner(`Optimizing packages${featureStr}...`) : null;
		spinner?.start();

		const preserve = String(opts.preserveDays ?? 90);
		const lruPackages = String(opts.lruMaxPackages ?? 1000);
		const lruSize = String(opts.lruMaxSizeBytes ?? 10000000000);

		const args = [
			'optimize',
			'--preserve-days', preserve,
			'--lru-max-packages', lruPackages,
			'--lru-max-size-bytes', lruSize,
		];

		if (opts.enableSymlinking) args.push('--enable-symlinking');
		if (opts.enableMl) args.push('--enable-ml');
		if (opts.paths?.length) args.push('--paths', ...opts.paths);

		const res = await runCore(args);

		if (res.code !== 0) {
			spinner?.fail('Optimization failed');
			if (!g.quiet) logger.error(res.stderr || 'Optimize failed');
			process.exit(res.code);
		}

		spinner?.succeed('Optimization complete');
		output(res.stdout, format, 'optimize');
	});

program
	.command('symlink')
	.description('Execute symlinking for duplicate packages across projects')
	.option('-p, --paths <paths...>', 'Paths to process', [])
	.action(async (opts, cmd) => {
		const g = cmd.parent?.opts?.() || {};
		const format = (g.format || 'table') as OutputFormat;

		// Check for Windows symlink capability
		if (process.platform === 'win32') {
			console.log(chalk.yellow('⚠ Note: Symlinking on Windows requires Administrator privileges or Developer Mode.'));
		}

		const spinner = !g.quiet && format === 'table' ? new Spinner('Creating symlinks...') : null;
		spinner?.start();

		const args = ['symlink'];
		if (opts.paths?.length) args.push('--paths', ...opts.paths);

		const res = await runCore(args);

		if (res.code !== 0) {
			spinner?.fail('Symlinking failed');
			if (!g.quiet) {
				logger.error(res.stderr || 'Symlink failed');
				if (process.platform === 'win32' && res.stderr?.includes('symlink')) {
					console.log(chalk.yellow('\n💡 Tip: Enable Developer Mode in Windows Settings > For Developers'));
					console.log(chalk.gray('   Or run this command as Administrator'));
				}
			}
			process.exit(res.code);
		}

		spinner?.succeed('Symlinking complete');
		output(res.stdout, format, 'symlink');
	});

program.parse(process.argv);
