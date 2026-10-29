#!/usr/bin/env bun

import process, { chdir, exit } from "node:process";
import { exec } from "tinyexec";

const SHA_REGEX = /^[0-9a-f]{40,64}$/iu;
const ZERO_SHA_REGEX = /^0+$/u;
const REF_SPLIT_REGEX = /\s+/u;

async function runGitAsync(...parameters: ReadonlyArray<string>): Promise<string> {
	const result = await exec("git", [...parameters], {
		nodeOptions: { stdio: ["ignore", "pipe", "ignore"] },
		throwOnError: true,
	});

	return result.stdout.trim();
}

async function resolveRepoRootAsync(): Promise<string> {
	try {
		return await runGitAsync("rev-parse", "--show-toplevel");
	} catch {
		return process.cwd();
	}
}

function parseRefRecord(line: string): [localRef: string, localSha: string, remoteRef: string, remoteSha: string] {
	const [localRef = "", localSha = "", remoteRef = "", remoteSha = "", extra] = line.split(REF_SPLIT_REGEX);

	if (extra !== undefined || remoteRef.length === 0 || !SHA_REGEX.test(localSha) || !SHA_REGEX.test(remoteSha)) {
		process.stderr.write("Invalid pre-push ref record.\n");
		exit(1);
	}

	return [localRef, localSha, remoteRef, remoteSha];
}

function isDeletion(localSha: string): boolean {
	return ZERO_SHA_REGEX.test(localSha);
}

function isNewRef(remoteSha: string): boolean {
	return ZERO_SHA_REGEX.test(remoteSha);
}

async function resolveCommitAsync(sha: string): Promise<string> {
	try {
		// oxlint-disable-next-line unicorn-js/no-incorrect-template-string-interpolation -- intentional sha peel
		return await runGitAsync("rev-parse", "--verify", `${sha}^{commit}`);
	} catch (error) {
		process.stderr.write("Unable to resolve commit.\n");

		throw error;
	}
}

async function resolveRangeAsync(localSha: string, remoteSha: string): Promise<string> {
	const localCommit = await resolveCommitAsync(localSha);
	if (isNewRef(remoteSha)) return localCommit;

	try {
		// oxlint-disable-next-line unicorn-js/no-incorrect-template-string-interpolation -- intentional sha peel
		const remoteCommit = await runGitAsync("rev-parse", "--verify", `${remoteSha}^{commit}`);

		return `${remoteCommit}..${localCommit}`;
	} catch {
		return localCommit;
	}
}

async function scanRefAsync(localSha: string, remoteSha: string): Promise<number> {
	const range = await resolveRangeAsync(localSha, remoteSha);
	const result = await exec(
		"betterleaks",
		["git", "--config", ".betterleaks.toml", "--redact", "--no-banner", "--verbose", `--log-opts=${range}`],
		{
			nodeOptions: { stdio: "inherit" },
		},
	);

	return result.exitCode ?? 1;
}

async function scanManualRunAsync(): Promise<number> {
	const localCommit = await resolveCommitAsync("HEAD");
	let range = localCommit;

	try {
		const upstreamCommit = await runGitAsync("rev-parse", "--verify", "@{upstream}^{commit}");
		range = `${upstreamCommit}..${localCommit}`;
	} catch {
		// No upstream: scan full HEAD history.
	}

	process.stderr.write(`No hook stdin; scanning ${range} for a manual run.\n`);

	const result = await exec(
		"betterleaks",
		["git", "--config", ".betterleaks.toml", "--redact", "--no-banner", "--verbose", `--log-opts=${range}`],
		{
			nodeOptions: { stdio: "inherit" },
		},
	);

	return result.exitCode ?? 1;
}

chdir(await resolveRepoRootAsync());

const input = process.stdin.isTTY ? "" : await Bun.stdin.text();
const lines = input.split("\n");

let exitStatus = 0;
let sawRefRecord = false;

for (const line of lines) {
	const trimmed = line.trim();

	if (trimmed.length === 0) continue;
	const [, localSha, , remoteSha] = parseRefRecord(trimmed);
	sawRefRecord = true;

	// oxlint-disable-next-line no-await-in-loop -- refs scan sequentially so output stays readable
	const status = isDeletion(localSha) ? 0 : await scanRefAsync(localSha, remoteSha);

	if (status !== 0) {
		exitStatus = status;
	}
}

if (!sawRefRecord) {
	const status = await scanManualRunAsync();

	if (status !== 0) {
		exitStatus = status;
	}
}

exit(exitStatus);
