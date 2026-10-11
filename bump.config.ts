import { defineConfig } from "bumpp";
import { exec } from "tinyexec";

const VERSION_GO = "internal/version/version.go";

async function getUnpushedTagsAsync(): Promise<ReadonlyArray<string>> {
	const { stdout } = await exec("git", ["push", "--tags", "--dry-run", "--porcelain"], {
		throwOnError: true,
	});

	const unpushedTags = new Array<string>();
	let size = 0;

	for (const line of stdout.split("\n")) {
		if (!line.startsWith("*\trefs/tags/")) continue;
		unpushedTags[size++] = line.split("\t")[1]?.split(":")[0]?.slice("refs/tags/".length) ?? line;
	}

	// Same push target as bumpp's `git push --tags`. Porcelain marks refs the
	// remote doesn't have yet with a leading `*`.
	return unpushedTags;
}

// bumpp skips its own clean-tree check when `all` is set, then commits with
// `git add --all`. Check here so the release commit holds only the bump.
const { stdout: dirtyFiles } = await exec("git", ["status", "--porcelain"], { throwOnError: true });
if (dirtyFiles.trim().length > 0) {
	throw new Error(`git working tree is not clean; commit or stash first:\n${dirtyFiles}`);
}

// bumpp reads the current version from package.json; refuse to start if
// version.go, the platform packages, or the protocol versions have drifted.
await exec("node", ["scripts/validate-release.cjs"], { throwOnError: true });

// bumpp pushes with `git push --tags`, and any pushed `v*` tag starts
// release.yml. Refuse to start unless the new tag is the only one that will go
// out.
const strayTags = await getUnpushedTagsAsync();
if (strayTags.length > 0) {
	throw new Error(
		`local tags missing from the remote would be pushed with the release: ${strayTags.join(", ")}\n` +
			"push or delete them first (git tag -d <tag>)",
	);
}

const configuration = defineConfig({
	// Safe because the clean-tree check above runs first: everything dirty at
	// commit time came from this bump.
	all: true,
	commit: "chore(release): prepare v%s",
	// bumpp only rewrites `version` in package.json and does a plain string
	// replace in version.go; `execute` handles the rest of the lockstep set.
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
