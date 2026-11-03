#!/usr/bin/env node

// Runs two rotor binaries on copies of one project and diffs the resulting trees.
//
// Usage:
//   node tools/parity/main.ts <baseline-binary> <candidate-binary> <project-dir>
//     [--allowlist <file>] [--work-dir <dir>] [--copy-node-modules]
//     [-- <rotor args>]
//
// Rotor args default to `build`. Each binary runs inside its own copy of the
// project, so the outputs land in separate out dirs. The top-level node_modules
// is symlinked into each copy unless --copy-node-modules is given. Exits 1 on any
// difference the allowlist does not cover, or if the two binaries exit with
// different codes.

import { spawnSync } from "node:child_process";
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { argv, exit, stderr, stdout } from "node:process";
import { parseArgs } from "node:util";

import { applyAllowlist, diffTrees, parseAllowlist } from "./diff.ts";

const here = import.meta.dirname;
const separator = argv.indexOf("--");
const ownArguments = separator === -1 ? argv.slice(2) : argv.slice(2, separator);
const rotorArguments = separator === -1 ? ["build"] : argv.slice(separator + 1);

const { positionals, values } = parseArgs({
	allowPositionals: true,
	args: ownArguments,
	options: {
		allowlist: { default: path.resolve(here, "../../testdata/rust-parity-allowlist.txt"), type: "string" },
		"copy-node-modules": { default: false, type: "boolean" },
		"work-dir": { type: "string" },
	},
});

const [baselineBinary, candidateBinary, projectArgument] = positionals;
if (baselineBinary === undefined || candidateBinary === undefined || projectArgument === undefined) {
	stderr.write(
		"usage: node tools/parity/main.ts <baseline-binary> <candidate-binary> <project-dir> [options] [-- <rotor args>]\n",
	);
	exit(2);
}
const project = path.resolve(projectArgument);
const keepWorkDirectory = values["work-dir"] !== undefined;
const workRoot =
	values["work-dir"] === undefined
		? mkdtempSync(path.join(tmpdir(), "rotor-parity-"))
		: path.resolve(values["work-dir"]);

function prepare(name: string): string {
	const destination = path.join(workRoot, name);
	rmSync(destination, { force: true, recursive: true });
	mkdirSync(destination, { recursive: true });
	cpSync(project, destination, {
		filter: (source) => values["copy-node-modules"] || source !== path.join(project, "node_modules"),
		recursive: true,
		verbatimSymlinks: true,
	});
	if (!values["copy-node-modules"] && existsSync(path.join(project, "node_modules"))) {
		symlinkSync(path.join(project, "node_modules"), path.join(destination, "node_modules"), "dir");
	}
	return destination;
}

function run(binary: string, directory: string): number {
	const result = spawnSync(path.resolve(binary), rotorArguments, {
		cwd: directory,
		stdio: ["ignore", "inherit", "inherit"],
	});
	return result.status ?? 1;
}

try {
	const baselineDirectory = prepare("baseline");
	const candidateDirectory = prepare("candidate");
	const baselineStatus = run(baselineBinary, baselineDirectory);
	const candidateStatus = run(candidateBinary, candidateDirectory);

	const allowlist = existsSync(values.allowlist) ? parseAllowlist(readFileSync(values.allowlist, "utf8")) : [];
	const report = applyAllowlist(diffTrees(baselineDirectory, candidateDirectory), allowlist);

	for (const difference of report.unlisted) {
		stdout.write(`${difference.kind.padEnd(15)} ${difference.path}\n`);
	}
	for (const entry of report.unusedEntries) {
		stderr.write(`note: allowlist line ${entry.line} (${entry.pattern}) matched no difference\n`);
	}
	stdout.write(`${report.unlisted.length} unlisted, ${report.allowed.length} allowlisted differences\n`);

	let failed = report.unlisted.length > 0;
	if (baselineStatus !== candidateStatus) {
		stderr.write(`exit codes differ: baseline ${baselineStatus}, candidate ${candidateStatus}\n`);
		failed = true;
	}
	exit(failed ? 1 : 0);
} finally {
	if (!keepWorkDirectory) {
		rmSync(workRoot, { force: true, recursive: true });
	}
}
