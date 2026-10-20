#!/usr/bin/env node
// Writes GitHub release notes into CHANGELOG.md and
// documentation/src/content/releases/.
//
// Usage:
// node scripts/sync-release-notes.ts             Rebuild CHANGELOG.md and every
// release file. node scripts/sync-release-notes.ts --tag vX.Y.Z Write one release
// file and insert or replace that release in CHANGELOG.md.
//
// Options:
// --repo owner/name   Repository to read releases from (default:
// howmanyslop/sloptor).
//
// Requires an authenticated `gh` CLI (GH_TOKEN works in CI).

import { mkdir, readFile, writeFile } from "node:fs/promises";
import nodePath from "node:path";
import { parseArgs } from "node:util";
import { type } from "arktype";
import { exec } from "tinyexec";

const isReleaseSummary = type({
	name: "string",
	isDraft: "boolean",
	publishedAt: "string",
	tagName: "string",
}).readonly();
type ReleaseSummary = typeof isReleaseSummary.infer;

const isRelease = type({
	name: "string",
	body: "string",
	publishedAt: "string",
	tagName: "string",
	url: "string",
}).readonly();
type Release = typeof isRelease.infer;

interface ChangelogSection {
	readonly tag: string;
	readonly text: string;
}

interface ParsedChangelog {
	readonly references: Array<string>;
	readonly sections: Array<ChangelogSection>;
}

const ROOT_DIRECTORY = nodePath.join(import.meta.dirname, "..");
const CHANGELOG_PATH = nodePath.join(ROOT_DIRECTORY, "CHANGELOG.md");
const RELEASES_DIRECTORY = nodePath.join(ROOT_DIRECTORY, "documentation/src/content/releases");
const CHANGELOG_HEADER = "All notable changes to sloptor are documented here.\n";
const FULL_CHANGELOG_PATTERN = /^\*\*Full Changelog\*\*:.*$/gmu;
const BARE_FULL_CHANGELOG_PATTERN = /^(?<label>\*\*Full Changelog\*\*:) (?<url>https:\/\/\S+)$/gmu;
const HEADING_PATTERN = /^(?<hashes>#{1,5}) /gmu;
const SECTION_PATTERN = /^## \[(?<tag>[^\]]+)\]/u;
const LINK_REFERENCE_PATTERN = /^\[[^\]]+\]: \S+$/u;

const { values } = parseArgs({
	options: {
		repo: { default: "howmanyslop/sloptor", type: "string" },
		tag: { type: "string" },
	},
});

const repository = values.repo;

async function runGhCliAsync(...parameters: ReadonlyArray<string>): Promise<string> {
	const output = await exec("gh", parameters, { throwOnError: true });
	return output.stdout;
}

const isReleaseSummaries = isReleaseSummary.array().readonly();

async function listReleasesAsync(): Promise<ReadonlyArray<ReleaseSummary>> {
	const releasesString = await runGhCliAsync(
		"release",
		"list",
		"-R",
		repository,
		"--limit",
		"100",
		"--json",
		"tagName,name,publishedAt,isDraft",
	);

	const releases = isReleaseSummaries(JSON.parse(releasesString));
	if (releases instanceof type.errors) {
		throw new TypeError(`Failed to parse releases: ${releases.summary}`);
	}

	return releases
		.filter((release) => !release.isDraft && release.publishedAt.length > 0)
		.toSorted((left, right) => right.publishedAt.localeCompare(left.publishedAt));
}

async function viewReleaseAsync(tag: string): Promise<Release> {
	const releaseString = await runGhCliAsync(
		"release",
		"view",
		tag,
		"-R",
		repository,
		"--json",
		"body,name,publishedAt,url,tagName",
	);
	const release = isRelease(JSON.parse(releaseString));
	if (release instanceof type.errors) {
		throw new TypeError(`Failed to parse release ${tag}: ${release.summary}`);
	}

	return release;
}

function normalize(markdown: string): string {
	return markdown
		.replaceAll("\r\n", "\n")
		.split("\n")
		.map((line) => line.trimEnd())
		.join("\n")
		.trim();
}

function emptyNotes(release: Release): string {
	return `No release notes. See the [tag](${release.url}).`;
}

function renderReleaseFile(release: Release): string {
	const title = release.name.trim() || release.tagName;
	const body = normalize(release.body).replaceAll(BARE_FULL_CHANGELOG_PATTERN, "$<label> <$<url>>");
	return `# ${title}\n\n${body.length > 0 ? body : emptyNotes(release)}\n`;
}

function renderChangelogSection(release: Release): string {
	const date = release.publishedAt.slice(0, 10);
	// Nest release headings under the version heading.
	const body = normalize(release.body.replaceAll(FULL_CHANGELOG_PATTERN, "")).replaceAll(
		HEADING_PATTERN,
		"#$<hashes> ",
	);
	return `## [${release.tagName}] - ${date}\n\n${body.length > 0 ? body : emptyNotes(release)}\n`;
}

function linkReference(tag: string, previousTag: string | undefined): string {
	const url =
		previousTag === undefined
			? `https://github.com/${repository}/releases/tag/${tag}`
			: `https://github.com/${repository}/compare/${previousTag}...${tag}`;
	return `[${tag}]: ${url}`;
}

async function writeReleaseFileAsync(release: Release): Promise<void> {
	await mkdir(RELEASES_DIRECTORY, { recursive: true });
	await writeFile(nodePath.join(RELEASES_DIRECTORY, `${release.tagName}.md`), renderReleaseFile(release));
}

function renderChangelog(sections: ReadonlyArray<string>, references: ReadonlyArray<string>): string {
	const body = sections.map((section) => `${section.trimEnd()}\n`).join("\n");
	return `${CHANGELOG_HEADER}\n${body}\n${references.join("\n")}\n`;
}

function parseChangelog(text: string): ParsedChangelog {
	const sections = new Array<{ readonly lines: Array<string>; readonly tag: string }>();
	const references = new Array<string>();
	let current: undefined | { readonly lines: Array<string>; readonly tag: string };
	for (const line of text.split("\n")) {
		const tag = SECTION_PATTERN.exec(line)?.groups?.tag;
		if (tag !== undefined) {
			current = { lines: [line], tag };
			sections.push(current);
		} else if (LINK_REFERENCE_PATTERN.test(line)) {
			current = undefined;
			references.push(line);
		} else {
			current?.lines.push(line);
		}
	}
	return {
		references,
		sections: sections.map((section) => ({ tag: section.tag, text: section.lines.join("\n").trim() })),
	};
}

async function readChangelogAsync(): Promise<string> {
	try {
		return await readFile(CHANGELOG_PATH, "utf8");
	} catch {
		return CHANGELOG_HEADER;
	}
}

async function syncAllAsync(): Promise<void> {
	const githubReleases = await listReleasesAsync();
	const releases = await Promise.all(githubReleases.map(async (release) => viewReleaseAsync(release.tagName)));

	const sections = new Array<string>();
	const references = new Array<string>();

	await Promise.all(
		releases.map(async (release, index) => {
			await writeReleaseFileAsync(release);
			sections.push(renderChangelogSection(release));
			references.push(linkReference(release.tagName, releases[index + 1]?.tagName));
		}),
	);

	await writeFile(CHANGELOG_PATH, renderChangelog(sections, references));
	console.log(`Wrote CHANGELOG.md and ${releases.length} release files.`);
}

async function syncTagAsync(tag: string): Promise<void> {
	const release = await viewReleaseAsync(tag);
	await writeReleaseFileAsync(release);

	const ordered = await listReleasesAsync();
	const index = ordered.findIndex((entry) => entry.tagName === tag);
	const previousTag = index === -1 ? undefined : ordered[index + 1]?.tagName;
	const { references, sections } = parseChangelog(await readChangelogAsync());
	const section = { tag, text: renderChangelogSection(release).trim() };
	const reference = linkReference(tag, previousTag);

	const sectionIndex = sections.findIndex((entry) => entry.tag === tag);
	if (sectionIndex === -1) sections.unshift(section);
	else sections[sectionIndex] = section;

	const referenceIndex = references.findIndex((line) => line.startsWith(`[${tag}]: `));
	if (referenceIndex === -1) references.unshift(reference);
	else references[referenceIndex] = reference;

	await writeFile(
		CHANGELOG_PATH,
		renderChangelog(
			sections.map((entry) => entry.text),
			references,
		),
	);
	console.log(`Wrote ${tag} to CHANGELOG.md and documentation/src/content/releases/${tag}.md.`);
}

// oxlint-disable-next-line unicorn/prefer-ternary -- no.
if (values.tag === undefined) await syncAllAsync();
else await syncTagAsync(values.tag);
