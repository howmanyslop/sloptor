#!/usr/bin/env node

// Usage: node tools/tsrs-mirror/main.ts [--source <tsrs checkout>] [--out <dir>]
//
// Without --source, fetches the pinned commit from GitHub into a temp directory.
// With --source, the checkout must be at the pinned commit.

import { execFileSync } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import process, { argv, exit } from "node:process";
import { parseArgs } from "node:util";

import { fetchSource, mirror, TSRS_COMMIT, TSRS_REPOSITORY, TSRS_TAG } from "./mirror.ts";

const here = import.meta.dirname;
const repositoryRoot = path.resolve(here, "../..");

const { values } = parseArgs({
	args: argv.slice(2),
	options: {
		out: { default: path.join(repositoryRoot, "third_party/tsrs"), type: "string" },
		source: { type: "string" },
	},
});

let fetched: string | undefined;
try {
	let { source } = values;
	if (source === undefined) {
		fetched = mkdtempSync(path.join(tmpdir(), "tsrs-source-"));
		fetchSource(TSRS_REPOSITORY, TSRS_COMMIT, fetched);
		source = fetched;
	} else {
		const head = execFileSync("git", ["rev-parse", "HEAD"], { cwd: source, encoding: "utf8" }).trim();
		if (head !== TSRS_COMMIT) {
			throw new Error(`${source} is at ${head}, expected ${TSRS_COMMIT}`);
		}
	}
	const result = mirror({
		commit: TSRS_COMMIT,
		out: path.resolve(values.out),
		patchesDirectory: path.join(here, "patches"),
		source,
		tag: TSRS_TAG,
	});
	process.stdout.write(
		`mirrored ${result.crates.length} crates and ${result.patches.length} patches into ${values.out}\n`,
	);
} catch (error) {
	process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
	exit(1);
} finally {
	if (fetched !== undefined) {
		rmSync(fetched, { force: true, recursive: true });
	}
}
