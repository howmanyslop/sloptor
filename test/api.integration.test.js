"use strict";

const assert = require("node:assert/strict");
const { execFileSync } = require("node:child_process");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const test = require("node:test");

const { createBuildSession } = require("..");

const repoRoot = path.resolve(__dirname, "..");
const temporaryRoot = fs.mkdtempSync(path.join(os.tmpdir(), "sloptor-api-test-"));
const executable = path.join(temporaryRoot, process.platform === "win32" ? "sloptor.exe" : "sloptor");

test.before(() => {
	execFileSync("go", ["build", "-trimpath", "-o", executable, "./cmd/rotor"], {
		cwd: repoRoot,
		stdio: "inherit",
	});
});

test.after(() => {
	fs.rmSync(temporaryRoot, { force: true, recursive: true });
});

test("one session builds a project repeatedly through one native server", async () => {
	const project = path.join(temporaryRoot, "project");
	fs.cpSync(path.join(__dirname, "fixtures", "single-project"), project, { recursive: true });

	const session = createBuildSession({ executable });
	try {
		const first = await session.build({ project });
		assert.deepEqual(first.diagnostics, []);
		assert.equal(first.ok, true);
		assert.equal(first.files, 1);
		assert.deepEqual(first.outputs, ["out/main.luau"]);
		assert.match(fs.readFileSync(path.join(project, "out", "main.luau"), "utf8"), /answer = 42/);

		fs.writeFileSync(path.join(project, "src", "main.ts"), "export const answer = 43;\n");
		const second = await session.build({ project });
		assert.equal(second.ok, true);
		assert.match(fs.readFileSync(path.join(project, "out", "main.luau"), "utf8"), /answer = 43/);

		fs.writeFileSync(path.join(project, "src", "main.ts"), "export const answer: string = 43;\n");
		const failed = await session.build({ project });
		assert.equal(failed.ok, false);
		assert.equal(failed.diagnostics.length, 1);
		assert.equal(failed.diagnostics[0].code, "TS2322");
		assert.equal(failed.diagnostics[0].file, "src/main.ts");
		assert.equal(failed.diagnostics[0].line, 1);
		assert.equal(failed.diagnostics[0].severity, "error");
		assert.match(failed.diagnostics[0].message, /not assignable to type 'string'/);
	} finally {
		await session.dispose();
	}

	await assert.rejects(session.build({ project }), { code: "SESSION_DISPOSED" });
});

test("one request builds a canonical union with project-owned results and CLI byte parity", async () => {
	const apiRoot = path.join(temporaryRoot, "multi-root-api");
	const cliRoot = path.join(temporaryRoot, "multi-root-cli");
	const fixture = path.join(__dirname, "fixtures", "multi-root");
	fs.cpSync(fixture, apiRoot, { recursive: true });
	fs.cpSync(fixture, cliRoot, { recursive: true });

	const productionConfig = path.join(apiRoot, "production", "tsconfig.json");
	const testsConfig = path.join(apiRoot, "tests", "tsconfig.json");
	const session = createBuildSession({ executable });
	let result;
	try {
		result = await session.build({
			roots: [productionConfig, path.join(apiRoot, "production", "..", "production", "tsconfig.json"), testsConfig],
		});
	} finally {
		await session.dispose();
	}

	assert.equal(result.ok, true);
	assert.deepEqual(result.diagnostics, []);
	assert.equal(result.projects.length, 3);
	assert.equal(new Set(result.projects.map((project) => project.config.toLowerCase())).size, 3);
	assert.deepEqual(result.projects.map((project) => project.status), ["success", "success", "success"]);
	assert.deepEqual(result.projects.map((project) => project.timings.counts.scheduledProjects), [1, 1, 1]);
	assert.deepEqual(result.telemetry, { scheduledProjects: 3, transformedProjects: 3, emittedProjects: 3 });
	for (const project of result.projects) {
		assert.deepEqual(project.blockers, []);
		assert.deepEqual(project.diagnostics, []);
		assert.equal(project.outputCount, project.outputs.length);
		assert.ok(project.outputCount > 0);
	}

	for (const root of ["production", "tests"]) {
		execFileSync(executable, ["build", "--build", "--project", path.join(cliRoot, root, "tsconfig.json")], {
			cwd: cliRoot,
			stdio: "pipe",
		});
	}
	assert.deepEqual(outputArtifacts(apiRoot), outputArtifacts(cliRoot));
});

test("a union failure owns one diagnostic and explicitly blocks every dependent", async () => {
	const root = path.join(temporaryRoot, "multi-root-failure");
	fs.cpSync(path.join(__dirname, "fixtures", "multi-root"), root, { recursive: true });
	fs.writeFileSync(path.join(root, "shared", "src", "value.ts"), "export const sharedValue: string = 40;\n");

	const session = createBuildSession({ executable });
	let result;
	try {
		result = await session.build({
			roots: [path.join(root, "production", "tsconfig.json"), path.join(root, "tests", "tsconfig.json")],
		});
	} finally {
		await session.dispose();
	}

	assert.equal(result.ok, false);
	assert.equal(result.diagnostics.length, 1);
	assert.equal(result.diagnostics[0].code, "TS2322");
	assert.deepEqual(result.projects.map((project) => project.status), ["failed", "blocked", "blocked"]);
	assert.equal(result.projects[0].diagnostics.length, 1);
	for (const project of result.projects.slice(1)) {
		assert.deepEqual(project.diagnostics, []);
		assert.deepEqual(project.blockers, [result.projects[0].config]);
	}
	assert.deepEqual(result.telemetry, { scheduledProjects: 3, transformedProjects: 0, emittedProjects: 0 });
});

test("an installed package resolves its native executable without an override", async () => {
	const installedPackage = path.join(temporaryRoot, "installed-package");
	fs.mkdirSync(path.join(installedPackage, "bin"), { recursive: true });
	for (const file of ["api.js", "api.d.ts", "package.json"]) {
		fs.copyFileSync(path.join(repoRoot, file), path.join(installedPackage, file));
	}
	const platform = { darwin: "darwin", linux: "linux", win32: "windows" }[process.platform];
	const architecture = { arm64: "arm64", x64: "amd64" }[process.arch];
	const extension = process.platform === "win32" ? ".exe" : "";
	fs.copyFileSync(executable, path.join(installedPackage, "bin", `sloptor-${platform}-${architecture}${extension}`));

	const installedClient = require(installedPackage);
	const project = path.join(temporaryRoot, "installed-project");
	fs.cpSync(path.join(__dirname, "fixtures", "single-project"), project, { recursive: true });
	const session = installedClient.createBuildSession();
	try {
		const result = await session.build({ project });
		assert.equal(result.ok, true);
		assert.deepEqual(result.outputs, ["out/main.luau"]);
	} finally {
		await session.dispose();
	}
});

function outputArtifacts(root) {
	const artifacts = {};
	for (const project of ["shared", "production", "tests"]) {
		const output = path.join(root, project, "out");
		for (const entry of fs.readdirSync(output, { recursive: true, withFileTypes: true })) {
			if (!entry.isFile() || entry.name === "rbxts.copyfiles.json") continue;
			const file = path.join(entry.parentPath, entry.name);
			artifacts[path.relative(root, file).split(path.sep).join("/")] = fs.readFileSync(file).toString("base64");
		}
	}
	return artifacts;
}
