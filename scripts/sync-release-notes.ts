#!/usr/bin/env node
// Writes GitHub release notes into CHANGELOG.md and documentation/src/content/releases/.
//
// Usage:
//   node scripts/sync-release-notes.ts             Rebuild CHANGELOG.md and every release file.
//   node scripts/sync-release-notes.ts --tag vX.Y.Z
//                                                  Write one release file and insert or replace
//                                                  that release in CHANGELOG.md.
//
// Options:
//   --repo owner/name   Repository to read releases from (default: howmanyslop/sloptor).
//
// Requires an authenticated `gh` CLI (GH_TOKEN works in CI).

import { execFileSync } from "node:child_process";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

interface ReleaseSummary {
	readonly isDraft: boolean;
	readonly name: string;
	readonly publishedAt: string;
	readonly tagName: string;
}

interface Release {
	readonly body: string;
	readonly name: string;
	readonly publishedAt: string;
	readonly tagName: string;
	readonly url: string;
}

interface ChangelogSection {
	readonly tag: string;
	readonly text: string;
}

interface ParsedChangelog {
	readonly references: Array<string>;
	readonly sections: Array<ChangelogSection>;
}

const ROOT_DIRECTORY = join(dirname(fileURLToPath(import.meta.url)), "..");
const CHANGELOG_PATH = join(ROOT_DIRECTORY, "CHANGELOG.md");
const RELEASES_DIRECTORY = join(ROOT_DIRECTORY, "documentation/src/content/releases");
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

function gh(...parameters: ReadonlyArray<string>): string {
	return execFileSync("gh", parameters, { encoding: "utf8", maxBuffer: 64 * 1024 * 1024 });
}

function listReleases(): Array<ReleaseSummary> {
	const releases = JSON.parse(
		gh("release", "list", "-R", repository, "--limit", "100", "--json", "tagName,name,publishedAt,isDraft"),
	) as Array<ReleaseSummary>;
	return releases
		.filter((release) => !release.isDraft && release.publishedAt.length > 0)
		.toSorted((left, right) => right.publishedAt.localeCompare(left.publishedAt));
}

function viewRelease(tag: string): Release {
	return JSON.parse(
		gh("release", "view", tag, "-R", repository, "--json", "body,name,publishedAt,url,tagName"),
	) as Release;
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

function writeReleaseFile(release: Release): void {
	mkdirSync(RELEASES_DIRECTORY, { recursive: true });
	writeFileSync(join(RELEASES_DIRECTORY, `${release.tagName}.md`), renderReleaseFile(release));
}

function renderChangelog(sections: ReadonlyArray<string>, references: ReadonlyArray<string>): string {
	const body = sections.map((section) => `${section.trimEnd()}\n`).join("\n");
	return `${CHANGELOG_HEADER}\n${body}\n${references.join("\n")}\n`;
}

function parseChangelog(text: string): ParsedChangelog {
	const sections = new Array<{ readonly lines: Array<string>; readonly tag: string }>();
	const references = new Array<string>();
	let current: { readonly lines: Array<string>; readonly tag: string } | undefined;
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

function readChangelog(): string {
	try {
		return readFileSync(CHANGELOG_PATH, "utf8");
	} catch {
		return CHANGELOG_HEADER;
	}
}

function syncAll(): void {
	const releases = listReleases().map((release) => viewRelease(release.tagName));
	const sections = new Array<string>();
	const references = new Array<string>();
	for (const [index, release] of releases.entries()) {
		writeReleaseFile(release);
		sections.push(renderChangelogSection(release));
		references.push(linkReference(release.tagName, releases[index + 1]?.tagName));
	}
	writeFileSync(CHANGELOG_PATH, renderChangelog(sections, references));
	console.log(`Wrote CHANGELOG.md and ${releases.length} release files.`);
}

function syncTag(tag: string): void {
	const release = viewRelease(tag);
	writeReleaseFile(release);

	const ordered = listReleases();
	const index = ordered.findIndex((entry) => entry.tagName === tag);
	const previousTag = index === -1 ? undefined : ordered[index + 1]?.tagName;

	const { references, sections } = parseChangelog(readChangelog());
	const section = { tag, text: renderChangelogSection(release).trim() };
	const reference = linkReference(tag, previousTag);

	const sectionIndex = sections.findIndex((entry) => entry.tag === tag);
	if (sectionIndex === -1) sections.unshift(section);
	else sections[sectionIndex] = section;

	const referenceIndex = references.findIndex((line) => line.startsWith(`[${tag}]: `));
	if (referenceIndex === -1) references.unshift(reference);
	else references[referenceIndex] = reference;

	writeFileSync(
		CHANGELOG_PATH,
		renderChangelog(
			sections.map((entry) => entry.text),
			references,
		),
	);
	console.log(`Wrote ${tag} to CHANGELOG.md and documentation/src/content/releases/${tag}.md.`);
}

if (values.tag === undefined) syncAll();
else syncTag(values.tag);
