/**
 * Output formatting utilities for PackagePurge CLI
 * Provides human-readable, JSON, and YAML output formats
 */
import chalk from 'chalk';
import YAML from 'yaml';

export type OutputFormat = 'table' | 'json' | 'yaml';

/**
 * Format bytes into a human-readable string
 */
export function formatBytes(bytes: number): string {
    if (bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
}

/**
 * Truncate a path to fit within maxLen characters
 */
export function truncatePath(pathStr: string, maxLen: number): string {
    if (pathStr.length <= maxLen) return pathStr;
    const ellipsis = '...';
    const start = Math.floor((maxLen - ellipsis.length) / 3);
    const end = maxLen - ellipsis.length - start;
    return pathStr.slice(0, start) + ellipsis + pathStr.slice(-end);
}

/**
 * Get color for a cleanup reason
 */
function getReasonColor(reason: string): chalk.Chalk {
    switch (reason.toLowerCase()) {
        case 'orphaned':
            return chalk.yellow;
        case 'old':
            return chalk.gray;
        case 'duplicate':
        case 'duplicate_symlink_candidate':
            return chalk.blue;
        case 'ml_predicted_unused':
            return chalk.magenta;
        case 'size_pressure':
            return chalk.red;
        default:
            return chalk.white;
    }
}

/**
 * Get human-readable reason text
 */
function formatReason(reason: string): string {
    switch (reason.toLowerCase()) {
        case 'orphaned':
            return 'Orphaned';
        case 'old':
            return 'Outdated';
        case 'duplicate':
            return 'Duplicate';
        case 'duplicate_symlink_candidate':
            return 'Symlink Candidate';
        case 'ml_predicted_unused':
            return 'ML: Unused';
        case 'size_pressure':
            return 'Size Pressure';
        default:
            return reason;
    }
}

/**
 * Extract package name and version from a path
 */
function extractPackageInfo(targetPath: string): { name: string; version: string } {
    // Try to extract from path like /path/to/node_modules/package-name or /path/to/cache/package@version
    const parts = targetPath.split(/[\/\\]/);
    const last = parts[parts.length - 1] || 'unknown';

    // Check for @scope/package format
    const nodeModulesIdx = parts.findIndex(p => p === 'node_modules');
    if (nodeModulesIdx !== -1 && nodeModulesIdx < parts.length - 1) {
        const pkgName = parts[nodeModulesIdx + 1];
        if (pkgName.startsWith('@') && nodeModulesIdx < parts.length - 2) {
            return { name: `${pkgName}/${parts[nodeModulesIdx + 2]}`, version: '-' };
        }
        return { name: pkgName, version: '-' };
    }

    // Try to parse name@version format
    const atIdx = last.lastIndexOf('@');
    if (atIdx > 0) {
        return { name: last.slice(0, atIdx), version: last.slice(atIdx + 1) };
    }

    return { name: last, version: '-' };
}

export interface PlanItem {
    target_path: string;
    estimated_size_bytes: number;
    reason: string;
}

export interface DryRunReport {
    items: PlanItem[];
    total_estimated_bytes: number;
}

export interface ScanOutput {
    packages: Array<{
        name: string;
        version: string;
        path: string;
        size_bytes: number;
    }>;
    projects: Array<{
        path: string;
        manager?: string;
    }>;
}

/**
 * Format scan output as a human-readable table
 */
export function formatScanAsTable(data: ScanOutput): void {
    console.log(chalk.bold.cyan('\n📦 Packages Found\n'));

    // Simple table without external dependency
    const header = `${'Package'.padEnd(30)} ${'Version'.padEnd(12)} ${'Size'.padEnd(10)} Path`;
    console.log(chalk.bold(header));
    console.log('─'.repeat(100));

    const sortedPackages = [...(data.packages || [])].sort((a, b) => b.size_bytes - a.size_bytes);
    const displayLimit = 50;
    const packagesToShow = sortedPackages.slice(0, displayLimit);

    for (const pkg of packagesToShow) {
        const name = (pkg.name || 'unknown').slice(0, 28).padEnd(30);
        const version = (pkg.version || '-').slice(0, 10).padEnd(12);
        const size = formatBytes(pkg.size_bytes).padEnd(10);
        const path = truncatePath(pkg.path, 45);
        console.log(`${chalk.green(name)} ${chalk.cyan(version)} ${chalk.yellow(size)} ${chalk.gray(path)}`);
    }

    if (sortedPackages.length > displayLimit) {
        console.log(chalk.gray(`\n... and ${sortedPackages.length - displayLimit} more packages`));
    }

    const totalSize = data.packages?.reduce((sum, p) => sum + p.size_bytes, 0) || 0;
    console.log(chalk.bold(`\n📊 Total: ${data.packages?.length || 0} packages, ${formatBytes(totalSize)}`));

    if (data.projects?.length) {
        console.log(chalk.bold.cyan(`\n📁 Projects Found: ${data.projects.length}`));
        for (const proj of data.projects.slice(0, 10)) {
            console.log(`   ${chalk.gray('•')} ${proj.path}`);
        }
        if (data.projects.length > 10) {
            console.log(chalk.gray(`   ... and ${data.projects.length - 10} more projects`));
        }
    }
}

/**
 * Format cleanup plan as a human-readable table
 */
export function formatPlanAsTable(data: DryRunReport): void {
    console.log(chalk.bold.cyan('\n🧹 Cleanup Plan\n'));

    if (!data.items?.length) {
        console.log(chalk.green('✓ No packages identified for cleanup!'));
        return;
    }

    // Group by reason
    const byReason = new Map<string, PlanItem[]>();
    for (const item of data.items) {
        const items = byReason.get(item.reason) || [];
        items.push(item);
        byReason.set(item.reason, items);
    }

    // Display by category
    for (const [reason, items] of byReason) {
        const colorFn = getReasonColor(reason);
        const reasonText = formatReason(reason);
        const categorySize = items.reduce((sum, i) => sum + i.estimated_size_bytes, 0);

        console.log(colorFn.bold(`\n${reasonText} (${items.length} packages, ${formatBytes(categorySize)})`));
        console.log('─'.repeat(80));

        const sorted = [...items].sort((a, b) => b.estimated_size_bytes - a.estimated_size_bytes);
        const toShow = sorted.slice(0, 10);

        for (const item of toShow) {
            const { name } = extractPackageInfo(item.target_path);
            const size = formatBytes(item.estimated_size_bytes).padEnd(10);
            console.log(`  ${colorFn('•')} ${name.padEnd(35)} ${chalk.yellow(size)} ${chalk.gray(truncatePath(item.target_path, 30))}`);
        }

        if (items.length > 10) {
            console.log(chalk.gray(`  ... and ${items.length - 10} more`));
        }
    }

    // Summary
    console.log(chalk.bold.green(`\n📊 Summary`));
    console.log(`   ${chalk.bold('Total packages:')} ${data.items.length}`);
    console.log(`   ${chalk.bold('Estimated savings:')} ${chalk.yellow.bold(formatBytes(data.total_estimated_bytes))}`);
}

/**
 * Format quarantine result
 */
export function formatQuarantineResult(data: any[]): void {
    console.log(chalk.bold.cyan('\n🗄️ Quarantine Results\n'));

    if (!data?.length) {
        console.log(chalk.yellow('No items were quarantined.'));
        return;
    }

    for (const rec of data) {
        console.log(chalk.green('✓') + ` Quarantined: ${chalk.cyan(rec.original_path || rec.id)}`);
        console.log(`  ${chalk.gray('ID:')} ${rec.id}`);
        console.log(`  ${chalk.gray('Size:')} ${formatBytes(rec.size_bytes || 0)}`);
    }

    const totalSize = data.reduce((sum, r) => sum + (r.size_bytes || 0), 0);
    console.log(chalk.bold.green(`\n✓ ${data.length} items quarantined, ${formatBytes(totalSize)} recoverable space`));
}

/**
 * Format rollback result
 */
export function formatRollbackResult(data: any): void {
    console.log(chalk.bold.cyan('\n↩️ Rollback Result\n'));

    if (data.status === 'ok') {
        console.log(chalk.green('✓') + ` Successfully rolled back: ${chalk.cyan(data.id)}`);
    } else {
        console.log(chalk.red('✗') + ` Rollback failed`);
    }
}

/**
 * Format symlink result
 */
export function formatSymlinkResult(data: any): void {
    console.log(chalk.bold.cyan('\n🔗 Symlink Results\n'));

    if (data.status === 'ok') {
        console.log(chalk.green('✓') + ` Successfully symlinked ${chalk.bold(data.symlinked_count)} packages`);
    } else {
        console.log(chalk.yellow('ℹ') + ` Symlink operation completed`);
        console.log(JSON.stringify(data, null, 2));
    }
}

/**
 * Format data as JSON
 */
export function formatAsJSON(data: any, pretty: boolean = true): string {
    return pretty ? JSON.stringify(data, null, 2) : JSON.stringify(data);
}

/**
 * Format data as YAML
 */
export function formatAsYAML(data: any): string {
    try {
        return YAML.stringify(data);
    } catch (e) {
        // Fallback to JSON if YAML conversion fails
        console.error(chalk.yellow('Warning: YAML conversion failed, falling back to JSON'));
        return JSON.stringify(data, null, 2);
    }
}

/**
 * Output data in the specified format
 */
export function output(
    data: string | object,
    format: OutputFormat,
    type: 'scan' | 'analyze' | 'optimize' | 'quarantine' | 'rollback' | 'symlink' = 'analyze'
): void {
    // Parse JSON string if needed
    let parsed: any;
    if (typeof data === 'string') {
        try {
            parsed = JSON.parse(data);
        } catch {
            // If not valid JSON, just print as-is for json/yaml, or show error for table
            if (format === 'table') {
                console.log(data);
                return;
            }
            parsed = data;
        }
    } else {
        parsed = data;
    }

    switch (format) {
        case 'table':
            switch (type) {
                case 'scan':
                    formatScanAsTable(parsed as ScanOutput);
                    break;
                case 'analyze':
                case 'optimize':
                    formatPlanAsTable(parsed as DryRunReport);
                    break;
                case 'quarantine':
                    formatQuarantineResult(parsed);
                    break;
                case 'rollback':
                    formatRollbackResult(parsed);
                    break;
                case 'symlink':
                    formatSymlinkResult(parsed);
                    break;
                default:
                    console.log(formatAsJSON(parsed));
            }
            break;
        case 'yaml':
            console.log(formatAsYAML(parsed));
            break;
        case 'json':
        default:
            console.log(formatAsJSON(parsed));
    }
}
