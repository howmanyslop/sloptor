"use strict";

const assert = require("node:assert/strict");
const { spawnSync } = require("node:child_process");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const test = require("node:test");

const repoRoot = path.resolve(__dirname, "..");

function validate(root = repoRoot, ...arguments_) {
	return spawnSync(process.execPath, ["scripts/validate-release.cjs", "--root", root, ...arguments_], {
		cwd: repoRoot,
		encoding: "utf8",
	});
}

function stageReleaseMetadata() {
	const root = fs.mkdtempSync(path.join(os.tmpdir(), "sloptor-release-metadata-"));
	for (const file of ["api.js", "package.json", "platform.js", "pnpm-lock.yaml"]) {
		fs.copyFileSync(path.join(repoRoot, file), path.join(root, file));
	}
	for (const directory of [path.join("internal", "version"), path.join("internal", "buildapi"), "packages"]) {
		fs.cpSync(path.join(repoRoot, directory), path.join(root, directory), { recursive: true });
	}
	return root;
}

function npmCommand(arguments_, options) {
	const bundledNpm = path.join(path.dirname(process.execPath), "node_modules", "npm", "bin", "npm-cli.js");
	const npmCli = process.env.npm_execpath || (fs.existsSync(bundledNpm) ? bundledNpm : undefined);
	return spawnSync(npmCli ? process.execPath : "npm", [...(npmCli ? [npmCli] : []), ...arguments_], {
		encoding: "utf8",
		...options,
	});
}

test("release validation accepts the checked-in package set", () => {
	const result = validate();
	assert.equal(result.status, 0, result.stderr);
	assert.match(result.stdout, /validated @rotor-rbx\/rotor@2\.6\.0 with 6 platform packages/);
});

test("release validation rejects platform-package version drift", () => {
	const root = stageReleaseMetadata();
	try {
		const manifestPath = path.join(root, "packages", "rotor-linux-x64", "package.json");
		const manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
		manifest.version = "2.5.0";
		fs.writeFileSync(manifestPath, `${JSON.stringify(manifest, null, "\t")}\n`);
		const result = validate(root);
		assert.notEqual(result.status, 0);
		assert.match(result.stderr, /rotor-linux-x64 version drift/);
	} finally {
		fs.rmSync(root, { force: true, recursive: true });
	}
});

test("release validation rejects client/server protocol drift and a mismatched tag", () => {
	const root = stageReleaseMetadata();
	try {
		const apiPath = path.join(root, "api.js");
		fs.writeFileSync(apiPath, fs.readFileSync(apiPath, "utf8").replace("const PROTOCOL_VERSION = 1;", "const PROTOCOL_VERSION = 2;"));
		let result = validate(root);
		assert.notEqual(result.status, 0);
		assert.match(result.stderr, /protocol drift/);

		fs.copyFileSync(path.join(repoRoot, "api.js"), apiPath);
		result = validate(root, "--tag", "v2.5.0");
		assert.notEqual(result.status, 0);
		assert.match(result.stderr, /tag v2\.5\.0 does not match release version v2\.6\.0/);
	} finally {
		fs.rmSync(root, { force: true, recursive: true });
	}
});

test("release staging produces the main package and all six executable packages", () => {
	const root = fs.mkdtempSync(path.join(os.tmpdir(), "sloptor-npm-release-"));
	const dist = path.join(root, "dist");
	const output = path.join(root, "npm");
	fs.mkdirSync(dist);
	const { PLATFORM_TARGETS } = require(path.join(repoRoot, "platform.js"));
	for (const target of Object.values(PLATFORM_TARGETS)) {
		const extension = target.goos === "windows" ? ".exe" : "";
		const executable = path.join(dist, `sloptor-v2.6.0-${target.goos}-${target.goarch}-bin${extension}`);
		if (target.packageName === `@rotor-rbx/rotor-${process.platform}-${process.arch}`) {
			fs.copyFileSync(process.execPath, executable);
		} else {
			fs.writeFileSync(executable, `fake ${target.goos}-${target.goarch}\n`);
		}
	}
	try {
		const result = spawnSync(
			process.execPath,
			["scripts/stage-npm-release.cjs", "--dist", dist, "--output", output],
			{ cwd: repoRoot, encoding: "utf8" },
		);
		assert.equal(result.status, 0, result.stderr);
		const release = JSON.parse(fs.readFileSync(path.join(output, "npm-release.json"), "utf8"));
		assert.equal(release.version, "2.6.0");
		assert.equal(release.packages.length, 7);
		assert.deepEqual(
			release.packages.map((entry) => entry.name).sort(),
			["@rotor-rbx/rotor", ...Object.values(PLATFORM_TARGETS).map((target) => target.packageName)].sort(),
		);
		for (const entry of release.packages) {
			assert.equal(fs.statSync(path.join(output, entry.tarball)).size > 0, true);
		}

		const consumer = path.join(root, "consumer");
		fs.mkdirSync(consumer);
		fs.writeFileSync(path.join(consumer, "package.json"), '{"name":"package-test","private":true}\n');
		const main = release.packages.find((entry) => entry.name === "@rotor-rbx/rotor");
		const platformPackage = release.packages.find(
			(entry) => entry.name === `@rotor-rbx/rotor-${process.platform}-${process.arch}`,
		);
		const install = npmCommand(
			[
				"install",
				"--ignore-scripts",
				"--omit=optional",
				"--no-package-lock",
				"--no-audit",
				"--no-fund",
				path.join(output, main.tarball),
				path.join(output, platformPackage.tarball),
			],
			{ cwd: consumer },
		);
		assert.equal(install.status, 0, install.stderr || install.stdout);
		const cli = path.join(consumer, "node_modules", "@rotor-rbx", "rotor", "bin", "rotor.js");
		const cliResult = spawnSync(
			process.execPath,
			[cli, "-e", "console.log('packed platform CLI'); process.exit(7)"],
			{ encoding: "utf8" },
		);
		assert.match(cliResult.stdout, /packed platform CLI/);
		assert.equal(cliResult.status, 7, cliResult.stderr);
	} finally {
		fs.rmSync(root, { force: true, recursive: true });
	}
});

test("the release version helper updates the main and every platform manifest", () => {
	const root = stageReleaseMetadata();
	try {
		const result = spawnSync(
			process.execPath,
			["scripts/sync-package-versions.cjs", "3.1.4", "--root", root],
			{ cwd: repoRoot, encoding: "utf8" },
		);
		assert.equal(result.status, 0, result.stderr);
		const main = JSON.parse(fs.readFileSync(path.join(root, "package.json"), "utf8"));
		assert.equal(main.version, "3.1.4");
		assert.deepEqual(new Set(Object.values(main.optionalDependencies)), new Set(["3.1.4"]));
		const lockfile = fs.readFileSync(path.join(root, "pnpm-lock.yaml"), "utf8");
		assert.equal((lockfile.match(/specifier: 3\.1\.4/g) ?? []).length, 6);
		for (const directory of fs.readdirSync(path.join(root, "packages"))) {
			const manifest = JSON.parse(fs.readFileSync(path.join(root, "packages", directory, "package.json"), "utf8"));
			assert.equal(manifest.version, "3.1.4");
		}
	} finally {
		fs.rmSync(root, { force: true, recursive: true });
	}
});
