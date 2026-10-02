#!/usr/bin/env node
"use strict";

const { spawnSync } = require("node:child_process");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");

function npmCommand(arguments_, options) {
	const bundledNpm = path.join(path.dirname(process.execPath), "node_modules", "npm", "bin", "npm-cli.js");
	// biome-ignore lint/style/noProcessEnv: npm exposes the active CLI through this standard environment variable.
	const npmCli = process.env.npm_execpath || (fs.existsSync(bundledNpm) ? bundledNpm : undefined);
	const result = spawnSync(npmCli ? process.execPath : "npm", [...(npmCli ? [npmCli] : []), ...arguments_], {
		encoding: "utf8",
		...options,
	});
	if (result.status !== 0) throw new Error(result.error?.message || result.stderr || result.stdout);
	return result;
}

function parseArguments(arguments_) {
	const options = {};
	for (let index = 0; index < arguments_.length; index++) {
		if (arguments_[index] === "--bundle") options.bundle = path.resolve(arguments_[++index]);
		else if (arguments_[index] === "--fixture") options.fixture = path.resolve(arguments_[++index]);
		else throw new Error(`unknown argument ${arguments_[index]}`);
	}
	if (!(options.bundle && options.fixture)) {
		throw new Error("usage: smoke-npm-release.cjs --bundle <directory> --fixture <project-directory>");
	}
	return options;
}

async function smokeNpmRelease({ bundle, fixture }) {
	const release = JSON.parse(fs.readFileSync(path.join(bundle, "npm-release.json"), "utf8"));
	const platformName = `@rotor-rbx/rotor-${process.platform}-${process.arch}`;
	const main = release.packages.find((entry) => entry.name === "@rotor-rbx/rotor");
	const platformPackage = release.packages.find((entry) => entry.name === platformName);
	if (!(main && platformPackage))
		throw new Error(`release bundle does not support ${process.platform}-${process.arch}`);

	const temporaryRoot = fs.mkdtempSync(path.join(os.tmpdir(), "sloptor-npm-smoke-"));
	try {
		const consumer = path.join(temporaryRoot, "consumer");
		fs.mkdirSync(consumer);
		fs.writeFileSync(path.join(consumer, "package.json"), '{"name":"sloptor-smoke","private":true}\n');
		npmCommand(
			[
				"install",
				"--ignore-scripts",
				"--omit=optional",
				"--no-package-lock",
				"--no-audit",
				"--no-fund",
				path.join(bundle, main.tarball),
				path.join(bundle, platformPackage.tarball),
			],
			{ cwd: consumer },
		);

		const installedMain = path.join(consumer, "node_modules", "@rotor-rbx", "rotor");
		const cli = spawnSync(process.execPath, [path.join(installedMain, "bin", "rotor.js"), "--version"], {
			encoding: "utf8",
		});
		if (cli.status !== 0) throw new Error(`installed CLI failed: ${cli.stderr || cli.stdout}`);
		if (cli.stdout.trim() !== release.version) {
			throw new Error(`installed CLI reported ${cli.stdout.trim()}, expected ${release.version}`);
		}

		const project = path.join(temporaryRoot, "project");
		fs.cpSync(fixture, project, { recursive: true });
		const { createBuildSession } = require(installedMain);
		const session = createBuildSession();
		try {
			const result = await session.build({ project });
			if (!(result.ok && result.outputs.includes("out/main.luau"))) {
				throw new Error(`installed API build failed: ${JSON.stringify(result)}`);
			}
		} finally {
			await session.dispose();
		}
		console.log(`smoke-tested API and CLI for ${platformName}@${release.version}`);
	} finally {
		fs.rmSync(temporaryRoot, { force: true, recursive: true });
	}
}

if (require.main === module) {
	smokeNpmRelease(parseArguments(process.argv.slice(2))).catch((error) => {
		console.error(error instanceof Error ? error.stack : error);
		process.exit(1);
	});
}

module.exports = { smokeNpmRelease };
