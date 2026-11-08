import { defineConfig } from "bumpp";
import { exec } from "tinyexec";

import { assertReleasableRepositoryAsync } from "./scripts/utilities/release-utilities";

// The sloptor CLI release; `nr release vscode` releases the extension without
// loading this file.
await assertReleasableRepositoryAsync();

await exec("node", ["scripts/validate-release.cjs"], { throwOnError: true });

const VERSION_RS = "crates/rotor_support/src/version.rs";

const configuration = defineConfig({
	all: true,
	commit: "chore(release): prepare v%s",
	async execute({ state }): Promise<void> {
		const { readFile } = await import("node:fs/promises");
		const { newVersion } = state;

		const versionRsText = await readFile(VERSION_RS, "utf8");

		if (!versionRsText.includes(`pub const VERSION: &str = "${newVersion}"`)) {
			throw new Error(`${VERSION_RS} was not bumped to ${newVersion}`);
		}

		await exec("node", ["scripts/sync-package-versions.cjs", newVersion], { throwOnError: true });
		await exec("bash", ["scripts/bump-header.sh"], { throwOnError: true });
		await exec("node", ["scripts/validate-release.cjs"], { throwOnError: true });
	},
	files: ["package.json", VERSION_RS],
	noGitCheck: false,
	printCommits: true,
	push: true,
	tag: "v%s",
});

export default configuration;
