import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { defineConfig } from "bumpp";

const VERSION_GO = "internal/version/version.go";

function run(command: string, ...arguments_: ReadonlyArray<string>): void {
	execFileSync(command, arguments_, { stdio: "inherit" });
}

function changedFiles(): ReadonlyArray<string> {
	return execFileSync("git", ["diff", "--name-only"], { encoding: "utf8" }).split("\n").filter(Boolean);
}

// bumpp reads the current version from package.json; refuse to start if
// version.go, the platform packages, or the protocol versions have drifted.
run("node", "scripts/validate-release.cjs");

const configuration = defineConfig({
	commit: "chore(release): prepare v%s",
	// bumpp only rewrites `version` in package.json and does a plain string
	// replace in version.go; `execute` handles the rest of the lockstep set.
	execute(operation) {
		const { newVersion } = operation.state;

		if (!readFileSync(VERSION_GO, "utf8").includes(`const Version = "${newVersion}"`)) {
			throw new Error(`${VERSION_GO} was not bumped to ${newVersion}`);
		}

		run("node", "scripts/sync-package-versions.cjs", newVersion);
		run("bash", "scripts/bump-header.sh");
		run("node", "scripts/validate-release.cjs");

		// bumpp commits only `updatedFiles`, so register everything the scripts
		// touched (platform manifests, lockfile, golden headers). The git check
		// guarantees the tree was clean beforehand.
		operation.update({ updatedFiles: [...new Set([...operation.state.updatedFiles, ...changedFiles()])] });
	},
	files: ["package.json", VERSION_GO],
	noGitCheck: false,
	printCommits: true,
	push: true,
	tag: "v%s",
});

export default configuration;
