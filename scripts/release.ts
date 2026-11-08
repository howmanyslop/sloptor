#!/usr/bin/env bun

import { existsSync } from "node:fs";
import { tmpdir } from "node:os";
import nodePath from "node:path";
import { exit } from "node:process";
import { EnumType } from "@cliffy/command";
import { loadBumpConfig, versionBump } from "bumpp";
import { exec } from "tinyexec";

import { createScriptCommand, parseScriptArgumentsAsync } from "$utilities/command-utilities";
import { assertReleasableRepositoryAsync } from "$utilities/release-utilities";

import type { VersionBumpOptions, VersionBumpRelease } from "bumpp";

const enum ReleaseTarget {
	Sloptor = "sloptor",
	VsCode = "vscode",
}

const ROOT_DIRECTORY = nodePath.join(import.meta.dirname, "..");
const VSCODE_DIRECTORY = nodePath.join(ROOT_DIRECTORY, "editors", "vscode");

// Keep in step with `.github/workflows/release-vscode.yaml`.
const VSCE = "@vscode/vsce@4.1.0";

const MARKETPLACE_VERSION_REGEXP = /^\d+\.\d+\.\d+$/u;
const VERSION_REGEXP = /^v?\d+\.\d+\.\d+(?:-[\d.a-z-]+)?(?:\+[\d.a-z-]+)?$/iu;
const RELEASE_TYPES: ReadonlySet<string> = new Set([
	"conventional",
	"conventional-prerelease",
	"major",
	"minor",
	"next",
	"patch",
	"premajor",
	"preminor",
	"prepatch",
	"prerelease",
	"prompt",
]);
const PRERELEASE_TYPES: ReadonlySet<string> = new Set([
	"conventional-prerelease",
	"premajor",
	"preminor",
	"prepatch",
	"prerelease",
]);

function isVersionBumpRelease(value: string): value is VersionBumpRelease {
	return RELEASE_TYPES.has(value) || VERSION_REGEXP.test(value);
}

/**
 * Runs after bumpp writes the new version and before it commits: packages the
 * extension the way CI does, so a broken build fails here instead of after the
 * tag is pushed.
 *
 * @param newVersion - The version bumpp wrote.
 * @rejects When the version is not a Marketplace version or packaging fails.
 */
async function verifyExtensionPackageAsync(newVersion: string): Promise<void> {
	// The Marketplace has no pre-release versions; a pre-release is a flag on a
	// plain version.
	if (!MARKETPLACE_VERSION_REGEXP.test(newVersion)) {
		throw new Error(
			`the Marketplace takes only major.minor.patch versions, not ${newVersion}; ` +
				"revert editors/vscode/package.json and pick another version",
		);
	}

	const outputPath = nodePath.join(tmpdir(), `sloptor-${newVersion}.vsix`);
	await exec("pnpm", ["dlx", VSCE, "package", "--no-dependencies", "--out", outputPath], {
		nodeOptions: { cwd: VSCODE_DIRECTORY, stdio: "inherit" },
		throwOnError: true,
	});
}

async function releaseSloptorAsync(overrides: Partial<VersionBumpOptions>): Promise<void> {
	await versionBump(await loadBumpConfig(overrides, ROOT_DIRECTORY));
}

async function releaseVsCodeAsync(overrides: Partial<VersionBumpOptions>): Promise<void> {
	await assertReleasableRepositoryAsync();
	// Not `loadBumpConfig`: the root config validates and syncs the CLI release.
	await versionBump({
		all: true,
		commit: "chore(release): vscode-v%s",
		confirm: true,
		cwd: VSCODE_DIRECTORY,
		execute: async ({ state }): Promise<void> => verifyExtensionPackageAsync(state.newVersion),
		files: ["package.json"],
		noGitCheck: false,
		push: true,
		tag: "vscode-v%s",
		...overrides,
	});
}

const { command, log } = createScriptCommand("1.0.0", "Releases the sloptor CLI or the VS Code extension with bumpp.");

command
	.type("release-target", new EnumType([ReleaseTarget.Sloptor, ReleaseTarget.VsCode]))
	.argument("[target:release-target]", "What to release (default: sloptor).")
	.argument("[release:string]", "Release type or version, like patch, minor or 1.2.3. Prompts when omitted.")
	.option("-y, --yes", "Skip the confirmation prompt.")
	.action(async ({ yes }, target, release) => {
		const releaseTarget = target ?? ReleaseTarget.Sloptor;
		if (releaseTarget === ReleaseTarget.VsCode && !existsSync(nodePath.join(VSCODE_DIRECTORY, "package.json"))) {
			log.fail("editors/vscode/package.json does not exist.");
			exit(1);
		}

		const overrides: Partial<VersionBumpOptions> = {};
		if (yes === true) {
			overrides.confirm = false;
		}

		if (release !== undefined) {
			if (!isVersionBumpRelease(release)) {
				log.fail(`"${release}" is not a release type or version, like patch, minor, major or 1.2.3.`);
				exit(1);
			}

			if (releaseTarget === ReleaseTarget.VsCode && (PRERELEASE_TYPES.has(release) || release.includes("-"))) {
				log.fail(`the Marketplace has no pre-release versions; "${release}" would make one.`);
				exit(1);
			}

			overrides.release = release;
		}

		await (releaseTarget === ReleaseTarget.VsCode ? releaseVsCodeAsync(overrides) : releaseSloptorAsync(overrides));
	});

await parseScriptArgumentsAsync(command);
