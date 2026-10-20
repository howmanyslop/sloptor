import { defineConfig } from "bumpp";
import { exec } from "tinyexec";

async function getUnpushedTagsAsync(): Promise<ReadonlyArray<string>> {
	const { stdout } = await exec("git", ["push", "--tags", "--dry-run", "--porcelain"], {
		throwOnError: true,
	});

	const unpushedTags = new Array<string>();
	let size = 0;

	for (const line of stdout.split("\n")) {
		if (!line.startsWith("*\trefs/tags/")) continue;
		unpushedTags[size++] = line.split("\t", 2)[1]?.split(":", 1)[0]?.slice("refs/tags/".length) ?? line;
	}

	// Same push target as bumpp's `git push --tags`. Porcelain marks refs the
	// remote doesn't have yet with a leading `*`.
	return unpushedTags;
}

const { stdout: dirtyFiles } = await exec("git", ["status", "--porcelain"], { throwOnError: true });
if (dirtyFiles.trim().length > 0) {
	throw new Error(`git working tree is not clean; commit or stash first:\n${dirtyFiles}`);
}

await exec("node", ["scripts/validate-release.cjs"], { throwOnError: true });

const strayTags = await getUnpushedTagsAsync();
if (strayTags.length > 0) {
	throw new Error(
		`local tags missing from the remote would be pushed with the release: ${strayTags.join(", ")}\n` +
			"push or delete them first (git tag -d <tag>)",
	);
}

const VERSION_GO = "internal/version/version.go";

const configuration = defineConfig({
	all: true,
	commit: "chore(release): prepare v%s",
	async execute({ state }): Promise<void> {
		const { readFile } = await import("node:fs/promises");
		const { newVersion } = state;

		const versionGoText = await readFile(VERSION_GO, "utf8");

		if (!versionGoText.includes(`const Version = "${newVersion}"`)) {
			throw new Error(`${VERSION_GO} was not bumped to ${newVersion}`);
		}

		await exec("node", ["scripts/sync-package-versions.cjs", newVersion], { throwOnError: true });
		await exec("bash", ["scripts/bump-header.sh"], { throwOnError: true });
		await exec("node", ["scripts/validate-release.cjs"], { throwOnError: true });
	},
	files: ["package.json", VERSION_GO],
	noGitCheck: false,
	printCommits: true,
	push: true,
	tag: "v%s",
});

export default configuration;
