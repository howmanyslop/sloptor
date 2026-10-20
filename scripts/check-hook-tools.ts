#!/usr/bin/env bun

import process, { exit } from "node:process";
import { EnumType } from "@cliffy/command";
import { exec } from "tinyexec";

import { createScriptCommand, parseScriptArgumentsAsync } from "$utilities/command-utilities";

interface RunResult {
	readonly code: number;
	readonly output: string;
}

async function runAsync(command: string, parameters: ReadonlyArray<string>, captureOutput = false): Promise<RunResult> {
	try {
		const result = exec(command, [...parameters], {
			nodeOptions: {
				stdio: captureOutput ? ["ignore", "pipe", "inherit"] : "inherit",
			},
		});
		const output = await result;
		return { code: output.exitCode ?? 1, output: captureOutput ? output.stdout : "" };
	} catch (error) {
		log.fail(error instanceof Error ? error.message : String(error));
		return { code: 1, output: "" };
	}
}

const enum HookTool {
	Gitleaks = "gitleaks",
	Shfmt = "shfmt",
	Typos = "typos",
}

async function runGitleaksAsync(files: ReadonlyArray<string>): Promise<void> {
	let next = 0;
	let failed = false;

	const workers = Array.from({ length: Math.min(8, files.length) }, async () => {
		while (next < files.length) {
			const file = files[next++];
			if (file === undefined) break;

			// oxlint-disable-next-line no-await-in-loop -- sequential to prevent overload
			const { code } = await runAsync("gitleaks", ["dir", "--redact", "--no-banner", "--log-level=error", file]);
			if (code !== 0) {
				console.error(`gitleaks failed: ${file}`);
				failed = true;
			}
		}
	});
	await Promise.all(workers);
	// oxlint-disable-next-line typescript/no-unnecessary-condition -- lol what
	if (failed) exit(1);
}
async function runShfmtAsync(files: ReadonlyArray<string>): Promise<void> {
	const { code, output } = await runAsync("shfmt", ["-i", "4", "-l", "--apply-ignore", ...files], true);
	if (output) process.stdout.write(output);
	if (code !== 0 || output.trim()) exit(1);
}
async function runTyposAsync(files: ReadonlyArray<string>): Promise<void> {
	const { code, output } = await runAsync("typos", ["--force-exclude", "--diff", ...files], true);
	if (output) process.stdout.write(output);
	if (code !== 0 || output.trim()) exit(1);
}

const { command, log } = createScriptCommand("1.0.0", "Runs hooks for `hk`.");

command
	.type("hook-tool", new EnumType([HookTool.Gitleaks, HookTool.Shfmt, HookTool.Typos]))
	.argument("<tool:hook-tool>", "The hook tool to check.")
	.argument("<files...:file>", "The files to check.")
	.action(async (_, hookTool, ...files) => {
		switch (hookTool) {
			case HookTool.Gitleaks: {
				await runGitleaksAsync(files);
				break;
			}

			case HookTool.Shfmt: {
				await runShfmtAsync(files);
				break;
			}

			case HookTool.Typos: {
				await runTyposAsync(files);
				break;
			}

			default: {
				log.fail(`Unknown hook tool: ${String(hookTool satisfies never)}`);
				exit(1);
			}
		}
	});

await parseScriptArgumentsAsync(command);
