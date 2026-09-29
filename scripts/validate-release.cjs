#!/usr/bin/env node
"use strict";

const fs = require("node:fs");
const path = require("node:path");

const CLIENT_PROTOCOL_PATTERN = /^const PROTOCOL_VERSION = (?<protocolVersion>[0-9]+);$/mu;
const GO_VERSION_PATTERN = /^const Version = "(?<version>[^"]+)"$/mu;
const SERVER_PROTOCOL_PATTERN = /^const ProtocolVersion = (?<protocolVersion>[0-9]+)$/mu;

function readJson(file) {
	return JSON.parse(fs.readFileSync(file, "utf8"));
}

function readMatch(file, pattern, label) {
	const match = fs.readFileSync(file, "utf8").match(pattern);
	if (!match) throw new Error(`could not read ${label} from ${file}`);
	return match[1];
}

function validateRelease({ root = path.resolve(__dirname, ".."), tag } = {}) {
	const packageJson = readJson(path.join(root, "package.json"));
	const codeVersion = readMatch(
		path.join(root, "internal", "version", "version.go"),
		GO_VERSION_PATTERN,
		"Go product version",
	);
	if (packageJson.version !== codeVersion) {
		throw new Error(`version drift: package.json=${packageJson.version}, internal/version=${codeVersion}`);
	}
	if (tag !== undefined && tag !== `v${codeVersion}`) {
		throw new Error(`tag ${tag} does not match release version v${codeVersion}`);
	}

	const clientProtocol = Number(
		readMatch(path.join(root, "api.js"), CLIENT_PROTOCOL_PATTERN, "client protocol version"),
	);
	const serverProtocol = Number(
		readMatch(
			path.join(root, "internal", "buildapi", "server.go"),
			SERVER_PROTOCOL_PATTERN,
			"server protocol version",
		),
	);
	if (clientProtocol !== serverProtocol) {
		throw new Error(`protocol drift: JavaScript client=${clientProtocol}, native server=${serverProtocol}`);
	}

	const platformModule = require(path.join(root, "platform.js"));
	const targets = Object.values(platformModule.PLATFORM_TARGETS);
	if (targets.length !== 6) throw new Error(`expected 6 supported platform targets, found ${targets.length}`);
	const expectedDependencies = targets.map((target) => target.packageName).sort();
	const actualDependencies = Object.keys(packageJson.optionalDependencies ?? {}).sort();
	if (JSON.stringify(actualDependencies) !== JSON.stringify(expectedDependencies)) {
		throw new Error(
			`optional platform package set drift: expected ${expectedDependencies.join(", ")}; found ${actualDependencies.join(", ") || "none"}`,
		);
	}

	for (const target of targets) {
		const dependencyVersion = packageJson.optionalDependencies[target.packageName];
		if (dependencyVersion !== codeVersion) {
			throw new Error(
				`${target.packageName} dependency must be exactly ${codeVersion}, found ${dependencyVersion}`,
			);
		}
		const escapedName = target.packageName.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&");
		const lockfile = fs.readFileSync(path.join(root, "pnpm-lock.yaml"), "utf8");
		const lockfileSpecifier = lockfile.match(
			new RegExp(` {6}'${escapedName}':\\r?\\n {8}specifier: ([^\\r\\n]+)`, "u"),
		)?.[1];
		if (lockfileSpecifier !== codeVersion) {
			throw new Error(
				`${target.packageName} lockfile specifier must be exactly ${codeVersion}, found ${lockfileSpecifier}`,
			);
		}
		const directory = target.packageName.slice("@rotor-rbx/".length);
		const manifest = readJson(path.join(root, "packages", directory, "package.json"));
		const expectedArchitecture = target.goarch === "amd64" ? "x64" : target.goarch;
		if (manifest.name !== target.packageName) {
			throw new Error(`platform manifest name drift: expected ${target.packageName}, found ${manifest.name}`);
		}
		if (manifest.version !== codeVersion) {
			throw new Error(`${target.packageName} version drift: expected ${codeVersion}, found ${manifest.version}`);
		}
		if (JSON.stringify(manifest.os) !== JSON.stringify([target.goos === "windows" ? "win32" : target.goos])) {
			throw new Error(`${target.packageName} has incorrect os restriction`);
		}
		if (JSON.stringify(manifest.cpu) !== JSON.stringify([expectedArchitecture])) {
			throw new Error(`${target.packageName} has incorrect cpu restriction`);
		}
	}

	return { protocolVersion: clientProtocol, targets, version: codeVersion };
}

function parseArguments(arguments_) {
	const options = {};
	for (let index = 0; index < arguments_.length; index++) {
		if (arguments_[index] === "--root") options.root = path.resolve(arguments_[++index]);
		else if (arguments_[index] === "--tag") options.tag = arguments_[++index];
		else throw new Error(`unknown argument ${arguments_[index]}`);
	}
	return options;
}

if (require.main === module) {
	try {
		const result = validateRelease(parseArguments(process.argv.slice(2)));
		console.log(
			`validated @rotor-rbx/rotor@${result.version} with ${result.targets.length} platform packages and protocol ${result.protocolVersion}`,
		);
	} catch (error) {
		console.error(error instanceof Error ? error.message : error);
		process.exit(1);
	}
}

module.exports = { validateRelease };
