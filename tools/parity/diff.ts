// Tree diffing and the reviewed allowlist for the Go-versus-Rust parity runner.

import { lstatSync, readdirSync, readFileSync, readlinkSync } from "node:fs";
import path from "node:path";

import type { Dirent } from "node:fs";

export type DifferenceKind = "changed" | "only-baseline" | "only-candidate";

export interface Difference {
	readonly kind: DifferenceKind;
	readonly path: string;
}

export interface TreeDiffOptions {
	/** Directory names skipped at any depth. */
	readonly ignoredDirectories?: ReadonlyArray<string>;
	/** File names skipped at any depth. */
	readonly ignoredFiles?: ReadonlyArray<string>;
}

/**
 * Mirrors `diff -r -x .rotor -x rbxts.copyfiles.json`, plus dependency and VCS
 * directories.
 */
export const DEFAULT_IGNORED_DIRECTORIES: ReadonlyArray<string> = [".git", ".rotor", "node_modules"];
export const DEFAULT_IGNORED_FILES: ReadonlyArray<string> = ["rbxts.copyfiles.json"];

type ResolvedOptions = Required<TreeDiffOptions>;

function classify(entry: Dirent, absolute: string, options: ResolvedOptions): string | undefined {
	if (entry.isSymbolicLink()) {
		return options.ignoredDirectories.includes(entry.name) || options.ignoredFiles.includes(entry.name)
			? undefined
			: `symlink:${readlinkSync(absolute)}`;
	}
	if (entry.isFile() && !options.ignoredFiles.includes(entry.name)) {
		return "file";
	}
	return undefined;
}

function collect(root: string, relative: string, options: ResolvedOptions, entries: Map<string, string>): void {
	const directory = relative.length === 0 ? root : path.join(root, relative);
	const children = readdirSync(directory, { withFileTypes: true });
	for (const entry of children) {
		const child = relative.length === 0 ? entry.name : `${relative}/${entry.name}`;
		if (entry.isDirectory()) {
			if (!options.ignoredDirectories.includes(entry.name)) {
				collect(root, child, options, entries);
			}
			continue;
		}
		const kind = classify(entry, path.join(root, child), options);
		if (kind !== undefined) {
			entries.set(child, kind);
		}
	}
}

function listEntries(root: string, options: ResolvedOptions): Map<string, string> {
	const entries = new Map<string, string>();
	if (lstatSync(root).isDirectory()) {
		collect(root, "", options, entries);
	}
	return entries;
}

function areSameBytes(baseline: string, candidate: string, relative: string): boolean {
	return readFileSync(path.join(baseline, relative)).equals(readFileSync(path.join(candidate, relative)));
}

function comparePaths(a: Difference, b: Difference): number {
	return a.path.localeCompare(b.path, "en");
}

/**
 * Compares two output trees byte for byte.
 *
 * @param baseline - The baseline tree root.
 * @param candidate - The candidate tree root.
 * @param options - Names to skip at any depth.
 * @returns The differences, sorted by path.
 */
export function diffTrees(baseline: string, candidate: string, options: TreeDiffOptions = {}): Array<Difference> {
	const resolved: Required<TreeDiffOptions> = {
		ignoredDirectories: options.ignoredDirectories ?? DEFAULT_IGNORED_DIRECTORIES,
		ignoredFiles: options.ignoredFiles ?? DEFAULT_IGNORED_FILES,
	};
	const left = listEntries(baseline, resolved);
	const right = listEntries(candidate, resolved);
	const differences: Array<Difference> = [];
	for (const [relative, kind] of left) {
		const other = right.get(relative);
		if (other === undefined) {
			differences.push({ kind: "only-baseline", path: relative });
		} else if (kind !== other || (kind === "file" && !areSameBytes(baseline, candidate, relative))) {
			differences.push({ kind: "changed", path: relative });
		}
	}
	for (const relative of right.keys()) {
		if (!left.has(relative)) {
			differences.push({ kind: "only-candidate", path: relative });
		}
	}
	return differences.toSorted(comparePaths);
}

const REGEXP_SPECIAL = /[$()+.[\\\]^{|}]/gu;
const LINE_BREAK = /\r?\n/u;

/**
 * Converts a glob (`*` within a segment, `**` across segments, `?` one
 * character) to a regular expression.
 *
 * @param glob - The glob to convert.
 * @returns An anchored expression matching whole paths.
 */
export function compileGlob(glob: string): RegExp {
	let source = "";
	for (let index = 0; index < glob.length; index += 1) {
		const character = glob.charAt(index);
		if (character === "*") {
			if (glob.charAt(index + 1) === "*") {
				index += 1;
				if (glob.charAt(index + 1) === "/") {
					index += 1;
					source += "(?:.*/)?";
				} else {
					source += ".*";
				}
			} else {
				source += "[^/]*";
			}
		} else if (character === "?") {
			source += "[^/]";
		} else {
			source += character.replaceAll(REGEXP_SPECIAL, String.raw`\$&`);
		}
	}
	return new RegExp(`^${source}$`, "u");
}

export interface AllowlistEntry {
	readonly line: number;
	readonly matcher: RegExp;
	readonly pattern: string;
}

function stripComment(raw: string): string {
	for (let index = 1; index < raw.length; index += 1) {
		if (raw.charAt(index) === "#" && (raw.charAt(index - 1) === " " || raw.charAt(index - 1) === "\t")) {
			return raw.slice(0, index).trim();
		}
	}
	return raw.trim();
}

/**
 * Parses an allowlist with one path or glob per line, relative to the project,
 * using `/` separators. Blank lines and lines starting with `#` are ignored,
 * and a trailing comment after whitespace and `#` is dropped.
 *
 * @param text - The allowlist file contents.
 * @returns One entry per pattern, with its 1-based line number.
 */
export function parseAllowlist(text: string): Array<AllowlistEntry> {
	const entries: Array<AllowlistEntry> = [];
	for (const [index, raw] of text.split(LINE_BREAK).entries()) {
		const pattern = stripComment(raw);
		if (pattern.length > 0 && !pattern.startsWith("#")) {
			entries.push({ line: index + 1, matcher: compileGlob(pattern), pattern });
		}
	}
	return entries;
}

export interface ParityReport {
	/** Differences the allowlist covers. */
	readonly allowed: ReadonlyArray<Difference>;
	/**
	 * Differences nothing in the allowlist covers: each one is a port bug until
	 * reviewed.
	 */
	readonly unlisted: ReadonlyArray<Difference>;
	/**
	 * Allowlist entries that matched nothing, which usually means they can be
	 * deleted.
	 */
	readonly unusedEntries: ReadonlyArray<AllowlistEntry>;
}

/**
 * Splits differences into allowlisted and unlisted ones.
 *
 * @param differences - The differences between the two trees.
 * @param allowlist - The parsed allowlist entries.
 * @returns The split, plus allowlist entries that matched nothing.
 */
export function applyAllowlist(
	differences: ReadonlyArray<Difference>,
	allowlist: ReadonlyArray<AllowlistEntry>,
): ParityReport {
	const used = new Set<AllowlistEntry>();
	const unlisted: Array<Difference> = [];
	const allowed: Array<Difference> = [];
	for (const difference of differences) {
		const match = allowlist.find((entry) => entry.matcher.test(difference.path));
		if (match === undefined) {
			unlisted.push(difference);
		} else {
			used.add(match);
			allowed.push(difference);
		}
	}
	return { allowed, unlisted, unusedEntries: allowlist.filter((entry) => !used.has(entry)) };
}
