#!/usr/bin/env node
"use strict";

const fs = require("node:fs");
const path = require("node:path");

function writeJson(file, value) {
	const temporary = `${file}.tmp`;
	fs.writeFileSync(temporary, `${JSON.stringify(value, null, "\t")}\n`);
	fs.renameSync(temporary, file);
}

function syncLockfile(root, packageNames, version) {
	const lockfile = path.join(root, "pnpm-lock.yaml");
	let contents = fs.readFileSync(lockfile, "utf8");
	for (const packageName of packageNames) {
		const escapedName = packageName.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
		const pattern = new RegExp(`( {6}'${escapedName}':\\r?\\n {8}specifier: )[^\\r\\n]+`);
		if (!pattern.test(contents)) throw new Error(`could not find ${packageName} in ${lockfile}`);
		contents = contents.replace(pattern, `$1${version}`);
	}
	const temporary = `${lockfile}.tmp`;
	fs.writeFileSync(temporary, contents);
	fs.renameSync(temporary, lockfile);
}

function syncPackageVersions(version, root = path.resolve(__dirname, "..")) {
	if (!/^[0-9]+\.[0-9]+\.[0-9]+(?:[.-][0-9A-Za-z.-]+)?$/.test(version)) {
		throw new Error(`invalid release version ${version}`);
	}
	const { PLATFORM_TARGETS } = require(path.join(root, "platform.js"));
	const mainPath = path.join(root, "package.json");
	const main = JSON.parse(fs.readFileSync(mainPath, "utf8"));
	const packageNames = Object.values(PLATFORM_TARGETS)
		.map((target) => target.packageName)
		.sort();
	main.version = version;
	main.optionalDependencies = Object.fromEntries(packageNames.map((packageName) => [packageName, version]));
	writeJson(mainPath, main);
	syncLockfile(root, packageNames, version);

	for (const target of Object.values(PLATFORM_TARGETS)) {
		const directory = target.packageName.slice("@rotor-rbx/".length);
		const manifestPath = path.join(root, "packages", directory, "package.json");
		const manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
		manifest.version = version;
		writeJson(manifestPath, manifest);
	}
}

function parseArguments(arguments_) {
	const [version, ...rest] = arguments_;
	let root = path.resolve(__dirname, "..");
	for (let index = 0; index < rest.length; index++) {
		if (rest[index] === "--root") root = path.resolve(rest[++index]);
		else throw new Error(`unknown argument ${rest[index]}`);
	}
	if (!version) throw new Error("usage: sync-package-versions.cjs <version> [--root <directory>]");
	return { root, version };
}

if (require.main === module) {
	try {
		const { root, version } = parseArguments(process.argv.slice(2));
		syncPackageVersions(version, root);
		console.log(`synchronized npm package versions to ${version}`);
	} catch (error) {
		console.error(error instanceof Error ? error.message : error);
		process.exit(1);
	}
}

module.exports = { syncPackageVersions };
