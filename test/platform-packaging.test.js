import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { pathToFileURL } from "node:url";

import packageJson from "../package.json" with { type: "json" };

const { version: packageVersion } = packageJson;

const repoRoot = path.resolve(import.meta.dirname, "..");
const temporaryRoot = fs.mkdtempSync(path.join(os.tmpdir(), "sloptor-platform-package-"));
const platformPackageName = `@rotor-rbx/rotor-${process.platform}-${process.arch}`;

function stageMain(name) {
	const root = path.join(temporaryRoot, name);
	fs.mkdirSync(path.join(root, "bin"), { recursive: true });
	fs.mkdirSync(path.join(root, "scripts"), { recursive: true });
	fs.mkdirSync(path.join(root, "tools", "sidecar"), { recursive: true });
	for (const file of ["api.js", "api.d.ts", "package.json", "platform.js"]) {
		fs.copyFileSync(path.join(repoRoot, file), path.join(root, file));
	}
	fs.copyFileSync(path.join(repoRoot, "bin", "rotor.js"), path.join(root, "bin", "rotor.js"));
	fs.copyFileSync(path.join(repoRoot, "scripts", "install.js"), path.join(root, "scripts", "install.js"));
	for (const file of ["index.js", "package.json"]) {
		fs.copyFileSync(path.join(repoRoot, "tools", "sidecar", file), path.join(root, "tools", "sidecar", file));
	}
	fs.cpSync(path.join(repoRoot, "tools", "sidecar", "lib"), path.join(root, "tools", "sidecar", "lib"), {
		recursive: true,
	});
	return root;
}

function stagePlatform(root, version = packageVersion, withBinary = true) {
	const packageRoot = path.join(root, "node_modules", ...platformPackageName.split("/"));
	fs.mkdirSync(path.join(packageRoot, "bin"), { recursive: true });
	fs.writeFileSync(
		path.join(packageRoot, "package.json"),
		`${JSON.stringify({ name: platformPackageName, version }, null, 2)}\n`,
	);
	const executable = path.join(packageRoot, "bin", process.platform === "win32" ? "sloptor.exe" : "sloptor");
	if (withBinary) {
		fs.copyFileSync(process.execPath, executable);
		if (process.platform !== "win32") fs.chmodSync(executable, 0o755);
	}
	return executable;
}

function importStaged(root, ...segments) {
	return import(pathToFileURL(path.join(root, ...segments)).href);
}

function runNode(args, options = {}) {
	return spawnSync(process.execPath, args, { encoding: "utf8", ...options });
}

test.after(() => fs.rmSync(temporaryRoot, { force: true, recursive: true }));

test("the API reports a missing optional platform package before starting a build", async () => {
	const root = stageMain("missing");
	const session = (await importStaged(root, "api.js")).createBuildSession();
	await assert.rejects(session.build({ project: root }), (error) => {
		assert.equal(error.code, "EXECUTABLE_NOT_FOUND");
		assert.match(error.message, new RegExp(platformPackageName));
		assert.match(error.message, /optional dependencies enabled/);
		return true;
	});
	await session.dispose();
});

test("the API rejects a platform package at a different version", async () => {
	const root = stageMain("mismatch");
	stagePlatform(root, "2.5.0");
	const session = (await importStaged(root, "api.js")).createBuildSession();
	await assert.rejects(session.build({ project: root }), (error) => {
		assert.equal(error.code, "VERSION_MISMATCH");
		assert.ok(error.message.includes(`@${packageVersion}, but found ${platformPackageName}@2.5.0`), error.message);
		return true;
	});
	await session.dispose();
});

test("the API rejects unsupported systems without starting a build", () => {
	const root = stageMain("unsupported");
	const script = `
Object.defineProperty(process, "platform", { value: "freebsd" });
const session = require(${JSON.stringify(root)}).createBuildSession();
session.build({ project: ${JSON.stringify(root)} }).then(
	() => process.exit(2),
	(error) => { console.log(JSON.stringify({ code: error.code, message: error.message })); }
);`;
	const result = runNode(["-e", script]);
	assert.equal(result.status, 0, result.stderr);
	const error = JSON.parse(result.stdout);
	assert.equal(error.code, "UNSUPPORTED_PLATFORM");
	assert.match(error.message, /freebsd/);
});

test("the CLI and compatibility installer use the same installed executable", async () => {
	const root = stageMain("cli");
	const executable = stagePlatform(root);
	const installer = await importStaged(root, "scripts", "install.js");
	assert.equal(installer.binaryPath(), executable);
	assert.equal(await installer.install({ quiet: true }), executable);

	const result = runNode(
		[path.join(root, "bin", "rotor.js"), "-e", "console.log('platform package CLI'); process.exit(7)"],
		{ cwd: root },
	);
	assert.match(result.stdout, /platform package CLI/);
	assert.equal(result.status, 7, result.stderr);
});
