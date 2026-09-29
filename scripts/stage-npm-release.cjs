#!/usr/bin/env node
"use strict";

const { spawnSync } = require("node:child_process");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");

const { validateRelease } = require("./validate-release.cjs");

function parseArguments(arguments_) {
	const options = {};
	for (let index = 0; index < arguments_.length; index++) {
		if (arguments_[index] === "--dist") options.dist = path.resolve(arguments_[++index]);
		else if (arguments_[index] === "--output") options.output = path.resolve(arguments_[++index]);
		else throw new Error(`unknown argument ${arguments_[index]}`);
	}
	if (!options.dist || !options.output)
		throw new Error("usage: stage-npm-release.cjs --dist <directory> --output <directory>");
	return options;
}

function npmPack(packageRoot, output) {
	const bundledNpm = path.join(path.dirname(process.execPath), "node_modules", "npm", "bin", "npm-cli.js");
	const npmCli = process.env.npm_execpath || (fs.existsSync(bundledNpm) ? bundledNpm : undefined);
	const command = npmCli ? process.execPath : "npm";
	const arguments_ = [...(npmCli ? [npmCli] : []), "pack", packageRoot, "--pack-destination", output, "--json"];
	const result = spawnSync(command, arguments_, {
		encoding: "utf8",
		shell: false,
	});
	if (result.status !== 0) {
		throw new Error(
			`npm pack failed for ${packageRoot}: ${result.error?.message || result.stderr || result.stdout}`,
		);
	}
	const packed = JSON.parse(result.stdout);
	if (!Array.isArray(packed) || packed.length !== 1) throw new Error(`unexpected npm pack output for ${packageRoot}`);
	return packed[0];
}

function releaseEntry(packed) {
	return {
		integrity: packed.integrity,
		name: packed.name,
		tarball: path.basename(packed.filename),
		version: packed.version,
	};
}

function stageNpmRelease({ dist, output, root = path.resolve(__dirname, "..") }) {
	const release = validateRelease({ root });
	fs.mkdirSync(output, { recursive: true });
	for (const file of fs.readdirSync(output)) {
		if (file.endsWith(".tgz") || file === "npm-release.json") fs.rmSync(path.join(output, file));
	}

	const temporaryRoot = fs.mkdtempSync(path.join(os.tmpdir(), "sloptor-npm-stage-"));
	const packages = [];
	try {
		for (const target of release.targets) {
			const directory = target.packageName.slice("@rotor-rbx/".length);
			const packageRoot = path.join(temporaryRoot, directory);
			const executableName = target.goos === "windows" ? "sloptor.exe" : "sloptor";
			const sourceName = `sloptor-v${release.version}-${target.goos}-${target.goarch}-bin${target.goos === "windows" ? ".exe" : ""}`;
			const source = path.join(dist, sourceName);
			if (!fs.statSync(source).isFile()) throw new Error(`missing release executable ${source}`);

			fs.mkdirSync(path.join(packageRoot, "bin"), { recursive: true });
			for (const file of ["README.md", "LICENSE"])
				fs.copyFileSync(path.join(root, file), path.join(packageRoot, file));
			fs.copyFileSync(
				path.join(root, "packages", directory, "package.json"),
				path.join(packageRoot, "package.json"),
			);
			const executable = path.join(packageRoot, "bin", executableName);
			fs.copyFileSync(source, executable);
			fs.chmodSync(executable, 0o755);

			const packed = npmPack(packageRoot, output);
			if (!packed.files.some((file) => file.path === `bin/${executableName}`)) {
				throw new Error(`${target.packageName} tarball does not contain bin/${executableName}`);
			}
			packages.push(releaseEntry(packed));
		}

		const main = npmPack(root, output);
		for (const file of [
			"api.js",
			"api.d.ts",
			"platform.js",
			"bin/rotor.js",
			"tools/sidecar/index.js",
			"tools/sidecar/lib/diagnostics.js",
			"tools/sidecar/lib/plugins.js",
			"tools/sidecar/lib/session.js",
		]) {
			if (!main.files.some((entry) => entry.path === file)) {
				throw new Error(`main package tarball does not contain ${file}`);
			}
		}
		packages.push(releaseEntry(main));
	} finally {
		fs.rmSync(temporaryRoot, { force: true, recursive: true });
	}

	const manifest = { packages, protocolVersion: release.protocolVersion, version: release.version };
	fs.writeFileSync(path.join(output, "npm-release.json"), `${JSON.stringify(manifest, null, 2)}\n`);
	return manifest;
}

if (require.main === module) {
	try {
		const manifest = stageNpmRelease(parseArguments(process.argv.slice(2)));
		console.log(`staged ${manifest.packages.length} npm packages for ${manifest.version}`);
	} catch (error) {
		console.error(error instanceof Error ? error.message : error);
		process.exit(1);
	}
}

module.exports = { stageNpmRelease };
