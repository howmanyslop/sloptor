#!/usr/bin/env node

// Kept as a compatibility subpath for consumers that called install() or
// binaryPath() before platform executables moved to optional dependencies.
// Resolution is local-only; this module never downloads or writes a binary.

import fs from "node:fs";
import { fileURLToPath } from "node:url";

import pkg from "../package.json" with { type: "json" };
import { PLATFORM_TARGETS, releaseBinaryName, resolvePlatformBinary } from "../platform.js";

const MIN_BINARY_SIZE = 1024 * 1024;

function binaryPath() {
	return resolvePlatformBinary();
}

function assetUrl() {
	const target = PLATFORM_TARGETS[`${process.platform}-${process.arch}`];
	if (!target) return undefined;
	return `https://github.com/howmanyslop/sloptor/releases/download/v${pkg.version}/${releaseBinaryName(pkg.version, target)}`;
}

async function install() {
	return binaryPath();
}

export { install, binaryPath, assetUrl, MIN_BINARY_SIZE };

function isEntryPoint() {
	if (!process.argv[1]) return false;
	try {
		return fs.realpathSync(process.argv[1]) === fileURLToPath(import.meta.url);
	} catch {
		return false;
	}
}

if (isEntryPoint()) {
	install()
		.then((executable) => console.log(executable))
		.catch((error) => {
			console.error(error instanceof Error ? error.message : error);
			process.exit(1);
		});
}
