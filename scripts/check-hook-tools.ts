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
	Deadcode = "deadcode",
	Shfmt = "shfmt",
	Typos = "typos",
}

/**
 * Fails on any output, since deadcode exits 0 when it reports unreachable
 * functions.
 *
 * @param packages - The Go package patterns to analyze.
 */
async function runDeadcodeAsync(packages: ReadonlyArray<string>): Promise<void> {
	const { code, output } = await runAsync(
		"deadcode",
		["-test", "-filter=^rotor/(internal|cmd|tools)", ...packages],
		true,
	);
	if (output) process.stdout.write(output);
	if (code !== 0 || output.trim()) exit(1);
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
	.type("hook-tool", new EnumType([HookTool.Deadcode, HookTool.Shfmt, HookTool.Typos]))
	.argument("<tool:hook-tool>", "The hook tool to check.")
	.argument("[files...:file]", "The files (or, for deadcode, Go packages) to check.")
	.action(async (_, hookTool, ...files) => {
		if (hookTool === HookTool.Deadcode) {
			await runDeadcodeAsync(files);
			return;
		}

		// tag-only pushes and empty diffs hand hk no files; shfmt/typos would
		// scan the cwd
		if (files.length === 0) return;

		switch (hookTool) {
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
