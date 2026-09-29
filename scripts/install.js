#!/usr/bin/env node
"use strict";

// Kept as a compatibility subpath for consumers that called install() or
// binaryPath() before platform executables moved to optional dependencies.
// Resolution is local-only; this module never downloads or writes a binary.

const path = require("node:path");

const pkg = require(path.join(__dirname, "..", "package.json"));
const { PLATFORM_TARGETS, releaseBinaryName, resolvePlatformBinary } = require("../platform.js");

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

module.exports = { install, binaryPath, assetUrl, MIN_BINARY_SIZE };

if (require.main === module) {
	install()
		.then((executable) => console.log(executable))
		.catch((error) => {
			console.error(error instanceof Error ? error.message : error);
			process.exit(1);
		});
}
