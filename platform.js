"use strict";

const fs = require("node:fs");
const path = require("node:path");

const mainPackage = require("./package.json");

const PLATFORM_TARGETS = Object.freeze({
	"darwin-arm64": Object.freeze({
		packageName: "@rotor-rbx/rotor-darwin-arm64",
		executable: "bin/sloptor",
		goos: "darwin",
		goarch: "arm64",
	}),
	"darwin-x64": Object.freeze({
		packageName: "@rotor-rbx/rotor-darwin-x64",
		executable: "bin/sloptor",
		goos: "darwin",
		goarch: "amd64",
	}),
	"linux-arm64": Object.freeze({
		packageName: "@rotor-rbx/rotor-linux-arm64",
		executable: "bin/sloptor",
		goos: "linux",
		goarch: "arm64",
	}),
	"linux-x64": Object.freeze({
		packageName: "@rotor-rbx/rotor-linux-x64",
		executable: "bin/sloptor",
		goos: "linux",
		goarch: "amd64",
	}),
	"win32-arm64": Object.freeze({
		packageName: "@rotor-rbx/rotor-win32-arm64",
		executable: "bin/sloptor.exe",
		goos: "windows",
		goarch: "arm64",
	}),
	"win32-x64": Object.freeze({
		packageName: "@rotor-rbx/rotor-win32-x64",
		executable: "bin/sloptor.exe",
		goos: "windows",
		goarch: "amd64",
	}),
});

class PlatformBinaryError extends Error {
	constructor(code, message, options) {
		super(message, options);
		this.name = "PlatformBinaryError";
		this.code = code;
	}
}

function reinstallMessage(packageName, version) {
	return `Reinstall @rotor-rbx/rotor@${version} with optional dependencies enabled, or install ${packageName}@${version} explicitly.`;
}

function platformExecutableName(target) {
	return path.posix.basename(target.executable);
}

function releaseBinaryName(version, target) {
	return `sloptor-v${version}-${target.goos}-${target.goarch}-bin${target.goos === "windows" ? ".exe" : ""}`;
}

function resolvePlatformBinary(options = {}) {
	const platform = options.platform ?? process.platform;
	const architecture = options.arch ?? process.arch;
	const packageRoot = options.packageRoot ?? __dirname;
	const expectedVersion = options.expectedVersion ?? mainPackage.version;
	const target = PLATFORM_TARGETS[`${platform}-${architecture}`];
	if (!target) {
		throw new PlatformBinaryError(
			"UNSUPPORTED_PLATFORM",
			`Sloptor does not support ${platform}-${architecture}. Supported targets are macOS, Linux, and Windows on x64 or arm64. Build from source with "go build ./cmd/rotor".`,
		);
	}

	let manifestPath;
	try {
		manifestPath = require.resolve(`${target.packageName}/package.json`, { paths: [packageRoot] });
	} catch (cause) {
		throw new PlatformBinaryError(
			"EXECUTABLE_NOT_FOUND",
			`The platform package ${target.packageName}@${expectedVersion} is not installed for ${platform}-${architecture}. ${reinstallMessage(target.packageName, expectedVersion)}`,
			{ cause },
		);
	}

	let platformPackage;
	try {
		platformPackage = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
	} catch (cause) {
		throw new PlatformBinaryError(
			"EXECUTABLE_NOT_FOUND",
			`Could not read ${target.packageName}'s package manifest at ${manifestPath}. ${reinstallMessage(target.packageName, expectedVersion)}`,
			{ cause },
		);
	}

	if (platformPackage.name !== target.packageName || platformPackage.version !== expectedVersion) {
		throw new PlatformBinaryError(
			"VERSION_MISMATCH",
			`Sloptor requires ${target.packageName}@${expectedVersion}, but found ${platformPackage.name ?? target.packageName}@${platformPackage.version ?? "unknown"}. ${reinstallMessage(target.packageName, expectedVersion)}`,
		);
	}

	const executable = path.join(path.dirname(manifestPath), ...target.executable.split("/"));
	try {
		const stat = fs.statSync(executable);
		if (!stat.isFile()) throw new Error("path is not a file");
		fs.accessSync(executable, fs.constants.R_OK | (platform === "win32" ? 0 : fs.constants.X_OK));
	} catch (cause) {
		throw new PlatformBinaryError(
			"EXECUTABLE_NOT_FOUND",
			`The platform package ${target.packageName}@${expectedVersion} does not contain an executable Sloptor binary at ${executable}. ${reinstallMessage(target.packageName, expectedVersion)}`,
			{ cause },
		);
	}

	return executable;
}

module.exports = {
	PLATFORM_TARGETS,
	PlatformBinaryError,
	platformExecutableName,
	releaseBinaryName,
	resolvePlatformBinary,
};
