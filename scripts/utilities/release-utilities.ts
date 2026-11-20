import { exec } from "tinyexec";

const UNPUSHED_TAG_PREFIX = "*\trefs/tags/";

async function getUnpushedTagsAsync(): Promise<ReadonlyArray<string>> {
	const { stdout } = await exec("git", ["push", "--tags", "--dry-run", "--porcelain"], {
		throwOnError: true,
	});

	const unpushedTags = new Array<string>();
	let size = 0;

	for (const line of stdout.split("\n")) {
		if (!line.startsWith(UNPUSHED_TAG_PREFIX)) continue;
		unpushedTags[size++] = line.split("\t", 2)[1]?.split(":", 1)[0]?.slice("refs/tags/".length) ?? line;
	}

	// Same push target as bumpp's `git push --tags`. Porcelain marks refs the
	// remote doesn't have yet with a leading `*`.
	return unpushedTags;
}

/**
 * Refuses to release from a dirty tree, or with local tags bumpp's
 * `git push --tags` would publish alongside the release tag.
 *
 * @rejects When the tree is dirty or a local tag is missing from the remote.
 */
export async function assertReleasableRepositoryAsync(): Promise<void> {
	const { stdout: dirtyFiles } = await exec("git", ["status", "--porcelain"], { throwOnError: true });
	if (dirtyFiles.trim().length > 0) {
		throw new Error(`git working tree is not clean; commit or stash first:\n${dirtyFiles}`);
	}

	const strayTags = await getUnpushedTagsAsync();
	if (strayTags.length > 0) {
		throw new Error(
			`local tags missing from the remote would be pushed with the release: ${strayTags.join(", ")}\n` +
				"push or delete them first (git tag -d <tag>)",
		);
	}
}
