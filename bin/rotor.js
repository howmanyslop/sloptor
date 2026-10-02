#!/usr/bin/env node
// npm/bun shim for the sloptor CLI. The executable comes from the same exact-
// version optional platform package used by the JavaScript API.
"use strict";

const { spawnSync } = require("child_process");
const { resolvePlatformBinary } = require("../platform.js");

function main() {
	const result = spawnSync(resolvePlatformBinary(), process.argv.slice(2), { stdio: "inherit" });
	if (result.error) throw result.error;
	if (result.signal) {
		// Re-raise the child's fatal signal so callers see the same termination.
		process.kill(process.pid, result.signal);
		return;
	}
	process.exit(result.status === null ? 1 : result.status);
}

try {
	main();
} catch (error) {
	console.error(error instanceof Error ? error.message : error);
	process.exit(1);
}
