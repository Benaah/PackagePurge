#!/usr/bin/env node

import { Command } from 'commander';
import chalk from 'chalk';
import { logger } from '../utils/logger';
import { runCore, runCoreStreaming, StreamProgress } from '../utils/core-utils';
import { output, OutputFormat } from '../utils/formatter';
import { loadConfig, detectWorkspace, mergeWithCliOptions, generateExampleConfig, PackagePurgeConfig } from '../utils/config';

// Load configuration early
const { config: loadedConfig, source: configSource } = loadConfig();

// Enhanced spinner with progress tracking
class Spinner {
	private frames = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
	private current = 0;
	private interval: NodeJS.Timeout | null = null;
	private text: string;
	private progressCount = 0;
	private progressType = '';

	constructor(text: string) {
		this.text = text;
	}

	start(): void {
		process.stderr.write('\x1B[?25l'); // Hide cursor
		this.interval = setInterval(() => {
			const progressStr = this.progressCount > 0
				? chalk.dim(` (${this.progressCount} ${this.progressType}s found)`)
				: '';
			process.stderr.write(`\r${chalk.cyan(this.frames[this.current])} ${this.text}${progressStr}`);
			this.current = (this.current + 1) % this.frames.length;
		}, 80);
	}

	update(text: string): void {
		this.text = text;
	}

	/**
	 * Update progress count for streaming operations
	 */
	progress(type: string, count: number): void {
		this.progressType = type;
		this.progressCount = count;
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

/**
 * Format file size for display
 */
function formatSize(bytes: number): string {
	if (bytes < 1024) return `${bytes} B`;
	if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
	if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
	return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

const program = new Command();
program
	.name('purge')
	.description('Intelligent package manager cache cleanup service with project-aware optimization')
	.version('2.0.0')
	.option('-q, --quiet', 'Minimal output', false)
	.option('-v, --verbose', 'Verbose logging', false)
	.option('-f, --format <format>', 'Output format: table|json|yaml', 'table');

program.hook('preAction', (_, actionCommand) => {
	const opts = actionCommand.optsWithGlobals();
	if (opts.verbose) logger.setLevel(0);
});

// Scan command with streaming support
program
	.command('scan')
	.description('Scan filesystem and output results')
	.option('-p, --paths <paths...>', 'Paths to scan', [])
	.option('--no-cache', 'Disable incremental caching')
	.action(async (opts, cmd) => {
		const g = cmd.parent?.opts?.() || {};
		const format = (g.format || 'table') as OutputFormat;
		const spinner = !g.quiet && format === 'table' ? new Spinner('Scanning for packages...') : null;

		spinner?.start();

		const args = ['scan', ...(opts.paths?.length ? ['--paths', ...opts.paths] : [])];

		// Use streaming for progress updates
		const res = await runCoreStreaming(args, (progress: StreamProgress) => {
			if (spinner && progress.type === 'package') {
				spinner.progress('package', progress.count);
			}
		});

		if (res.code !== 0) {
			spinner?.fail('Scan failed');
			if (!g.quiet) logger.error(res.stderr || 'Scan failed');
			process.exit(res.code);
		}

		spinner?.succeed('Scan complete');
		output(res.stdout, format, 'scan');
	});

// Analyze command (dry-run)
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

		const res = await runCoreStreaming(args, (progress: StreamProgress) => {
			if (spinner && progress.type === 'plan_item') {
				spinner.progress('cleanup target', progress.count);
			}
		});

		if (res.code !== 0) {
			spinner?.fail('Analysis failed');
			if (!g.quiet) logger.error(res.stderr || 'Analyze failed');
			process.exit(res.code);
		}

		spinner?.succeed('Analysis complete');
		output(res.stdout, format, 'analyze');
	});

// Clean command (quarantine)
program
	.command('clean')
	.description('Quarantine targets (Move-and-Delete transaction). Defaults to dry-run via analyze.')
	.option('-t, --targets <targets...>', 'Paths to quarantine (from analyze)')
	.option('--fast', 'Skip SHA256 verification for faster cleanup', false)
	.action(async (opts, cmd) => {
		const g = cmd.parent?.opts?.() || {};
		const format = (g.format || 'table') as OutputFormat;

		if (!opts.targets || !opts.targets.length) {
			if (!g.quiet) {
				console.log(chalk.yellow('⚠ No targets provided.'));
				console.log(chalk.gray('  Run `purge analyze` first to produce a cleanup plan.'));
				console.log(chalk.gray('  Then use: purge clean --targets <path1> <path2> ...'));
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

// Rollback command
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
				console.log(chalk.gray('  Use: purge rollback --latest'));
				console.log(chalk.gray('  Or:  purge rollback --id <quarantine-id>'));
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

// Optimize command
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

		const res = await runCoreStreaming(args, (progress: StreamProgress) => {
			if (spinner) {
				spinner.progress('optimization', progress.count);
			}
		});

		if (res.code !== 0) {
			spinner?.fail('Optimization failed');
			if (!g.quiet) logger.error(res.stderr || 'Optimize failed');
			process.exit(res.code);
		}

		spinner?.succeed('Optimization complete');
		output(res.stdout, format, 'optimize');
	});

// Symlink command
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

// Stats command - uses Rust core stats
program
	.command('stats')
	.description('Show quarantine and cache statistics')
	.action(async (opts, cmd) => {
		const g = cmd.parent?.opts?.() || {};
		const format = (g.format || loadedConfig.format || 'table') as OutputFormat;

		if (format === 'json' || format === 'yaml') {
			const res = await runCore(['stats']);
			if (res.code === 0) {
				output(res.stdout, format, 'stats');
			} else {
				console.error('Failed to get stats:', res.stderr);
				process.exit(res.code);
			}
		} else {
			console.log(chalk.bold('\n📊 PackagePurge Statistics\n'));

			// Show config source
			if (configSource) {
				console.log(chalk.cyan('Config loaded from:'), chalk.white(configSource));
			} else {
				console.log(chalk.cyan('Config:'), chalk.dim('Using defaults (no config file found)'));
			}

			// Detect workspace
			const workspace = detectWorkspace();
			if (workspace.type) {
				console.log(chalk.cyan('Workspace:'), chalk.white(`${workspace.type} (${workspace.packages.length} packages)`));
				console.log(chalk.cyan('Workspace root:'), chalk.white(workspace.root));
			}

			console.log();

			// Get stats from core
			const res = await runCore(['stats']);
			if (res.code === 0) {
				try {
					const stats = JSON.parse(res.stdout);
					console.log(chalk.bold('Quarantine:'));
					console.log(`  Entries: ${stats.quarantine?.total_entries || 0}`);
					console.log(`  Size: ${formatBytes(stats.quarantine?.total_size_bytes || 0)}`);
					console.log(`  Oldest: ${stats.quarantine?.oldest_entry_days || 0} days`);
					console.log();

					if (stats.scan_cache) {
						console.log(chalk.bold('Scan Cache:'));
						console.log(`  Entries: ${stats.scan_cache.total_entries}`);
						console.log(`  Cached size: ${formatBytes(stats.scan_cache.total_cached_size)}`);
					}
				} catch {
					console.log(res.stdout);
				}
			}

			console.log();
			console.log(chalk.dim('Locations:'));
			console.log(chalk.dim('  Quarantine: ~/.packagepurge/quarantine'));
			console.log(chalk.dim('  Cache: ~/.packagepurge/scan_cache.json'));
			console.log(chalk.dim('  Features: ~/.packagepurge/features.db'));
		}
	});

// Config command - show current configuration
program
	.command('config')
	.description('Show current configuration')
	.option('--json', 'Output as JSON')
	.action(async (opts) => {
		if (opts.json) {
			console.log(JSON.stringify(loadedConfig, null, 2));
		} else {
			console.log(chalk.bold('\n⚙️  PackagePurge Configuration\n'));

			if (configSource) {
				console.log(chalk.green('✓'), `Loaded from: ${chalk.cyan(configSource)}`);
			} else {
				console.log(chalk.yellow('!'), 'No config file found, using defaults');
				console.log(chalk.dim('  Run `purge init` to create a config file'));
			}

			console.log();
			console.log(chalk.bold('Settings:'));
			console.log(`  preserveDays: ${loadedConfig.preserveDays}`);
			console.log(`  enableSymlinking: ${loadedConfig.enableSymlinking}`);
			console.log(`  enableMl: ${loadedConfig.enableMl}`);
			console.log(`  lruMaxPackages: ${loadedConfig.lruMaxPackages}`);
			console.log(`  lruMaxSizeBytes: ${formatBytes(loadedConfig.lruMaxSizeBytes || 0)}`);
			console.log(`  format: ${loadedConfig.format}`);

			if (loadedConfig.paths && loadedConfig.paths.length > 0) {
				console.log(`  paths: ${loadedConfig.paths.join(', ')}`);
			}

			if (loadedConfig.exclude && loadedConfig.exclude.length > 0) {
				console.log(`  exclude: ${loadedConfig.exclude.join(', ')}`);
			}

			console.log();
			console.log(chalk.bold('Quarantine:'));
			console.log(`  maxSizeGb: ${loadedConfig.quarantine?.maxSizeGb}`);
			console.log(`  retentionDays: ${loadedConfig.quarantine?.retentionDays}`);
		}
	});

// Init command - generate example config
program
	.command('init')
	.description('Create a .packagepurgerc.yaml configuration file')
	.option('-f, --force', 'Overwrite existing config file')
	.action(async (opts) => {
		const configPath = '.packagepurgerc.yaml';

		if (!opts.force && require('fs').existsSync(configPath)) {
			console.log(chalk.yellow('⚠'), `Config file already exists: ${configPath}`);
			console.log(chalk.dim('  Use --force to overwrite'));
			process.exit(1);
		}

		const content = generateExampleConfig();
		require('fs').writeFileSync(configPath, content);

		console.log(chalk.green('✓'), `Created ${chalk.cyan(configPath)}`);
		console.log();
		console.log(chalk.dim('Edit this file to customize PackagePurge behavior.'));
		console.log(chalk.dim('Run `purge config` to see current settings.'));
	});

// Helper function for formatting bytes
function formatBytes(bytes: number): string {
	if (bytes < 1024) return `${bytes} B`;
	if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
	if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
	return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

program.parse(process.argv);

