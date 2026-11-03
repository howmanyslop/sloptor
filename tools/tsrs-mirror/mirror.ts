// Vendors the pinned tsrs release into `third_party/tsrs/`.
//
// tsrs is a Cargo workspace, so a bare copy of the crates rotor links would lose the
// `workspace = true` inheritance. The mirror copies the transitive closure of the
// needed crates, writes a trimmed workspace root that still carries
// `[workspace.package]`, `[workspace.dependencies]` and `[workspace.lints]`, then
// applies `patches/*.patch` in name order. Nobody edits `third_party/tsrs/` by hand.

import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
	cpSync,
	existsSync,
	mkdirSync,
	mkdtempSync,
	readdirSync,
	readFileSync,
	renameSync,
	rmSync,
	writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";

import type { StdioOptions } from "node:child_process";

export const TSRS_REPOSITORY = "https://github.com/oxc-project/tsrs.git";
export const TSRS_TAG = "v0.20.1";
export const TSRS_COMMIT = "77d3cddb44540734abc33a42ccca4d3f33bee527";

/**
 * The tsrs crates rotor links directly. The rest of the closure follows from
 * Cargo metadata.
 */
export const ROOT_CRATES: ReadonlyArray<string> = [
	"tsrs_ast",
	"tsrs_checker",
	"tsrs_compiler",
	"tsrs_contentmapper",
	"tsrs_core",
	"tsrs_diagnostics",
	"tsrs_parser",
	"tsrs_tsoptions",
	"tsrs_vfs",
];

export interface MetadataDependency {
	readonly name: string;
	readonly path?: null | string | undefined;
}

export interface MetadataPackage {
	readonly name: string;
	readonly dependencies: ReadonlyArray<MetadataDependency>;
}

const CARGO_MANIFEST = "Cargo.toml";
const QUIET = "--quiet";
const STDIO_QUIET: StdioOptions = ["ignore", "inherit", "inherit"];
const STDIO_CAPTURE: StdioOptions = ["ignore", "pipe", "inherit"];
const PATH_ENTRY = /^(?<name>tsrs_\w+)\s*=\s*\{\s*path\s*=/u;
const BLANK_RUNS = /\n{3,}/gu;
const IGNORED_COPY = /[\\/](?:target|node_modules|\.git)$/u;
const TYPESCRIPT_REFERENCE =
	/\[workspace\.metadata\.typescript\][^[]*?version\s*=\s*"(?<version>[^"]+)"[^[]*?commit\s*=\s*"(?<commit>[^"]+)"/u;

function compareNames(a: string, b: string): number {
	return a.localeCompare(b, "en");
}

/**
 * Returns the sorted names of `roots` and every workspace crate they depend
 * on. Dev-dependencies count: Cargo refuses to load a manifest whose path
 * dependencies are missing, whatever their kind.
 *
 * @param packages - The workspace packages from `cargo metadata`.
 * @param roots - The crates to start from.
 * @returns The sorted closure of crate names.
 */
export function computeClosure(packages: ReadonlyArray<MetadataPackage>, roots: ReadonlyArray<string>): Array<string> {
	const byName = new Map(packages.map((package_) => [package_.name, package_]));
	const seen = new Set<string>();
	const pending = [...roots];
	for (let name = pending.pop(); name !== undefined; name = pending.pop()) {
		if (seen.has(name)) {
			continue;
		}
		const package_ = byName.get(name);
		if (package_ === undefined) {
			throw new Error(`crate "${name}" is not a member of the tsrs workspace`);
		}
		seen.add(name);
		for (const dependency of package_.dependencies) {
			if ((dependency.path ?? "").length > 0 && byName.has(dependency.name)) {
				pending.push(dependency.name);
			}
		}
	}
	return [...seen].toSorted(compareNames);
}

const KEPT_SECTIONS = new Set([
	"[workspace.dependencies]",
	"[workspace.lints.clippy]",
	"[workspace.lints.rust]",
	"[workspace.metadata.typescript]",
	"[workspace.package]",
]);

interface ManifestSection {
	readonly body: Array<string>;
	/** Comment lines directly above the header; they travel with the section. */
	readonly comments: Array<string>;
	readonly header: string;
}

function isUncopiedCrate(line: string, keep: ReadonlySet<string>): boolean {
	const name = PATH_ENTRY.exec(line)?.groups?.name;
	return name !== undefined && !keep.has(name);
}

function renderSection(section: ManifestSection, keep: ReadonlySet<string>): Array<string> {
	const body = section.body.filter((line) => !isUncopiedCrate(line, keep));
	return [...section.comments, section.header, ...body];
}

function splitSections(text: string): Array<ManifestSection> {
	const sections: Array<ManifestSection> = [];
	let current: ManifestSection = { body: [], comments: [], header: "" };
	for (const line of text.split("\n")) {
		if (line.startsWith("[")) {
			sections.push(current);
			const comments: Array<string> = [];
			while (current.body.at(-1)?.startsWith("#") === true) {
				comments.unshift(current.body.pop() ?? "");
			}
			current = { body: [], comments, header: line.trim() };
		} else {
			current.body.push(line);
		}
	}
	sections.push(current);
	return sections;
}

/**
 * Builds the trimmed workspace root: only the sections inherited by member
 * crates, with path entries for crates that were not copied removed and the
 * member list pinned to the copied crates. Comments directly above a kept
 * section header travel with it.
 *
 * @param text - The upstream workspace `Cargo.toml`.
 * @param crates - The crates that were copied.
 * @returns The trimmed manifest text.
 */
export function trimWorkspaceManifest(text: string, crates: ReadonlyArray<string>): string {
	const keep = new Set(crates);
	const members = crates.map((name) => JSON.stringify(`crates/${name}`)).join(", ");
	const preamble = [
		"# Trimmed by tools/tsrs-mirror from the upstream tsrs workspace manifest.",
		"[workspace]",
		'resolver = "3"',
		`members = [${members}]`,
		"",
	];
	const kept = splitSections(text).flatMap((section) =>
		KEPT_SECTIONS.has(section.header) ? renderSection(section, keep) : [],
	);
	const out = [...preamble, ...kept];
	return `${out.join("\n").replaceAll(BLANK_RUNS, "\n\n").trimEnd()}\n`;
}

export function listPatches(directory: string): Array<string> {
	if (!existsSync(directory)) {
		return [];
	}
	return readdirSync(directory)
		.filter((name) => name.endsWith(".patch"))
		.toSorted(compareNames)
		.map((name) => path.join(directory, name));
}

export interface MirrorOptions {
	readonly commit: string;
	/** Destination directory, normally `third_party/tsrs`. */
	readonly out: string;
	readonly patchesDirectory: string;
	readonly roots?: ReadonlyArray<string>;
	/** A tsrs checkout at the pinned commit. */
	readonly source: string;
	readonly tag: string;
}

export interface MirrorResult {
	readonly crates: ReadonlyArray<string>;
	readonly patches: ReadonlyArray<string>;
}

interface CargoMetadata {
	readonly packages: ReadonlyArray<MetadataPackage>;
}

function isDependency(value: unknown): value is MetadataDependency {
	return (
		typeof value === "object" &&
		value !== null &&
		"name" in value &&
		typeof value.name === "string" &&
		(!("path" in value) || value.path === null || typeof value.path === "string")
	);
}

function isPackage(value: unknown): value is MetadataPackage {
	return (
		typeof value === "object" &&
		value !== null &&
		"name" in value &&
		typeof value.name === "string" &&
		"dependencies" in value &&
		Array.isArray(value.dependencies) &&
		value.dependencies.every(isDependency)
	);
}

/**
 * Checks only the `cargo metadata` fields this tool reads.
 *
 * @param value - The parsed JSON.
 * @returns Whether `value` has the fields this tool reads.
 */
function isCargoMetadata(value: unknown): value is CargoMetadata {
	return (
		typeof value === "object" &&
		value !== null &&
		"packages" in value &&
		Array.isArray(value.packages) &&
		value.packages.every(isPackage)
	);
}

/**
 * Reads the workspace members of a Cargo manifest, without dependencies.
 *
 * @param manifestPath - Path to the workspace `Cargo.toml`.
 * @returns The workspace packages and their dependency edges.
 */
export function readPackages(manifestPath: string): Array<MetadataPackage> {
	const stdout = execFileSync(
		"cargo",
		["metadata", "--format-version", "1", "--no-deps", "--offline", "--manifest-path", manifestPath],
		{ encoding: "utf8", maxBuffer: 256 * 1024 * 1024, stdio: STDIO_CAPTURE },
	);
	const metadata = JSON.parse(stdout);
	if (!isCargoMetadata(metadata)) {
		throw new TypeError("`cargo metadata` output has no packages with names and dependencies");
	}
	return [...metadata.packages];
}

function renderMirrorNotes(
	options: MirrorOptions,
	crates: ReadonlyArray<string>,
	patches: ReadonlyArray<string>,
	typescript: string,
): string {
	const patchLines = patches.map((patch) => {
		const digest = createHash("sha256").update(readFileSync(patch)).digest("hex").slice(0, 16);
		const name = patch.slice(patch.lastIndexOf("/") + 1);
		return `- \`${name}\` (sha256 \`${digest}\`)`;
	});
	return [
		"# tsrs mirror",
		"",
		"Generated by `tools/tsrs-mirror`. Do not edit by hand; change `tools/tsrs-mirror/patches/` and re-run the mirror.",
		"",
		`- Upstream: ${TSRS_REPOSITORY}`,
		`- Tag: \`${options.tag}\``,
		`- Commit: \`${options.commit}\``,
		`- TypeScript reference: ${typescript}`,
		"",
		"## Patches",
		"",
		...(patchLines.length > 0 ? patchLines : ["None."]),
		"",
		"## Crates",
		"",
		...crates.map((name) => `- \`${name}\``),
		"",
	].join("\n");
}

/**
 * Copies the closure into a staging directory, patches it there, and swaps it
 * into `out`.
 *
 * @param options - Where to read from, write to and what to apply.
 * @returns The copied crates and the applied patches.
 */
export function mirror(options: MirrorOptions): MirrorResult {
	const roots = options.roots ?? ROOT_CRATES;
	const crates = computeClosure(readPackages(path.join(options.source, CARGO_MANIFEST)), roots);

	const staging = mkdtempSync(path.join(tmpdir(), "tsrs-mirror-"));
	try {
		for (const name of crates) {
			cpSync(path.join(options.source, "crates", name), path.join(staging, "crates", name), {
				filter: (source) => !IGNORED_COPY.test(source),
				recursive: true,
			});
		}
		for (const file of ["LICENSE", "NOTICE"]) {
			cpSync(path.join(options.source, file), path.join(staging, file));
		}
		if (existsSync(path.join(options.source, "licenses"))) {
			cpSync(path.join(options.source, "licenses"), path.join(staging, "licenses"), { recursive: true });
		}
		const rootManifest = readFileSync(path.join(options.source, CARGO_MANIFEST), "utf8");
		writeFileSync(path.join(staging, CARGO_MANIFEST), trimWorkspaceManifest(rootManifest, crates));

		const patches = listPatches(options.patchesDirectory);
		for (const patch of patches) {
			execFileSync("git", ["apply", "-p1", "--whitespace=nowarn", patch], {
				cwd: staging,
				stdio: STDIO_QUIET,
			});
		}

		const typescript = TYPESCRIPT_REFERENCE.exec(rootManifest)?.groups;
		const typescriptRef =
			typescript === undefined ? "unknown" : `${typescript.version} (\`${typescript.commit?.slice(0, 12)}\`)`;
		writeFileSync(path.join(staging, "MIRROR.md"), renderMirrorNotes(options, crates, patches, typescriptRef));

		rmSync(options.out, { force: true, recursive: true });
		mkdirSync(path.join(options.out, ".."), { recursive: true });
		try {
			renameSync(staging, options.out);
		} catch {
			// Cross-device rename: copy instead.
			cpSync(staging, options.out, { recursive: true });
		}
		return { crates, patches };
	} finally {
		rmSync(staging, { force: true, recursive: true });
	}
}

/**
 * Fetches exactly `commit` (a full SHA) into `destination` with a shallow
 * fetch.
 */
export function fetchSource(repository: string, commit: string, destination: string): void {
	mkdirSync(destination, { recursive: true });
	function git(...arguments_: Array<string>): string {
		return execFileSync("git", arguments_, {
			cwd: destination,
			encoding: "utf8",
			stdio: STDIO_CAPTURE,
		});
	}
	git("init", QUIET);
	git("remote", "add", "origin", repository);
	git("fetch", QUIET, "--depth", "1", "origin", commit);
	git("checkout", QUIET, "--detach", "FETCH_HEAD");
	const head = git("rev-parse", "HEAD").trim();
	if (head !== commit) {
		throw new Error(`fetched ${head}, expected ${commit}`);
	}
}
