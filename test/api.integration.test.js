import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import { createRequire } from "node:module";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { pathToFileURL } from "node:url";

import { createBuildSession } from "../api.js";
import packageJson from "../package.json" with { type: "json" };

const { version: packageVersion } = packageJson;

const repoRoot = path.resolve(import.meta.dirname, "..");
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
	fs.cpSync(path.join(import.meta.dirname, "fixtures", "single-project"), project, { recursive: true });

	const session = createBuildSession({ executable });
	try {
		const first = await session.build({ project });
		assert.deepEqual(first.diagnostics, []);
		assert.equal(first.ok, true);
		assert.equal(first.files, 1);
		assert.deepEqual(first.outputs, [path.join(project, "out", "main.luau").split(path.sep).join("/")]);
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

test("project output ownership includes every cache artifact across no-change builds", async () => {
	const project = path.join(temporaryRoot, "complete-output-ownership");
	fs.cpSync(path.join(import.meta.dirname, "fixtures", "single-project"), project, { recursive: true });
	const configPath = path.join(project, "tsconfig.json");
	const config = JSON.parse(fs.readFileSync(configPath, "utf8"));
	Object.assign(config.compilerOptions, {
		declaration: true,
		declarationMap: true,
		incremental: true,
		sourceMap: true,
		tsBuildInfoFile: "out/cache.tsbuildinfo",
	});
	fs.writeFileSync(configPath, JSON.stringify(config));
	fs.writeFileSync(path.join(project, "src", "data.json"), '{"ok":true}\n');

	const expected = [
		"out/data.json",
		"out/globals.d.ts",
		"out/main.d.ts",
		"out/main.d.ts.map",
		"out/main.luau",
		"out/main.luau.map",
	];
	const session = createBuildSession({ executable });
	try {
		const first = await session.build({ project });
		const second = await session.build({ project });
		assert.deepEqual([first.projects[0].status, second.projects[0].status], ["success", "no-change"]);
		for (const result of [first, second]) {
			assert.deepEqual(result.projects[0].outputs, expected);
			assert.equal(result.projects[0].outputCount, expected.length);
			assert.deepEqual(
				result.outputs,
				expected.map((output) => path.join(project, output).split(path.sep).join("/")),
			);
		}
	} finally {
		await session.dispose();
	}
});

test("API transformer callbacks retain project state without spawning sidecars and match CLI bytes", async () => {
	const apiProject = path.join(temporaryRoot, "transformer-api");
	const cliProject = path.join(temporaryRoot, "transformer-cli");
	createTransformerProject(apiProject);
	createTransformerProject(cliProject);

	const session = createBuildSession({ executable, transformerCallbacks: true });
	try {
		const first = await session.build({ project: apiProject });
		assert.equal(first.ok, true);
		assert.equal(first.projects[0].timings.counts.sidecarSpawns ?? 0, 0);
		assert.match(fs.readFileSync(path.join(apiProject, "out", "main.luau"), "utf8"), /callback:start/);

		fs.writeFileSync(path.join(apiProject, "src", "main.ts"), 'export const phase = "next";\n');
		const second = await session.build({ project: apiProject });
		assert.equal(second.ok, true);
		assert.equal(second.projects[0].timings.counts.sidecarSpawns ?? 0, 0);
		assert.match(fs.readFileSync(path.join(apiProject, "out", "main.luau"), "utf8"), /callback:next/);
	} finally {
		await session.dispose();
	}

	fs.writeFileSync(path.join(cliProject, "src", "main.ts"), 'export const phase = "next";\n');
	execFileSync(executable, ["build", "--project", path.join(cliProject, "tsconfig.json")], {
		cwd: cliProject,
		env: { ...process.env, ROTOR_SIDECAR_PATH: path.join(repoRoot, "tools", "sidecar") },
		stdio: "pipe",
	});
	assert.deepEqual(outputArtifactsForProject(apiProject), outputArtifactsForProject(cliProject));
});

test("API sessions transform in server sidecars by default and match CLI bytes", async () => {
	const apiProject = path.join(temporaryRoot, "transformer-native-api");
	const cliProject = path.join(temporaryRoot, "transformer-native-cli");
	createTransformerProject(apiProject);
	createTransformerProject(cliProject);
	const pidFile = path.join(apiProject, "transformer.pid");
	fs.appendFileSync(
		path.join(apiProject, "transformer.js"),
		`require("fs").writeFileSync(${JSON.stringify(pidFile)}, String(process.pid));\n`,
	);
	const sidecarEnv = { ROTOR_SIDECAR_PATH: path.join(repoRoot, "tools", "sidecar") };

	const previousSidecarPath = process.env.ROTOR_SIDECAR_PATH;
	process.env.ROTOR_SIDECAR_PATH = sidecarEnv.ROTOR_SIDECAR_PATH;
	const session = createBuildSession({ executable });
	try {
		const result = await session.build({ project: apiProject });
		assert.equal(result.ok, true);
		assert.notEqual(Number(fs.readFileSync(pidFile, "utf8")), process.pid);
		assert.match(fs.readFileSync(path.join(apiProject, "out", "main.luau"), "utf8"), /callback:start/);
	} finally {
		await session.dispose();
		if (previousSidecarPath === undefined) delete process.env.ROTOR_SIDECAR_PATH;
		else process.env.ROTOR_SIDECAR_PATH = previousSidecarPath;
	}

	execFileSync(executable, ["build", "--project", path.join(cliProject, "tsconfig.json")], {
		cwd: cliProject,
		env: { ...process.env, ...sidecarEnv },
		stdio: "pipe",
	});
	assert.deepEqual(outputArtifactsForProject(apiProject), outputArtifactsForProject(cliProject));
});

test("API transformer plugins resolve project modules from outside the project like the CLI", async () => {
	const apiProject = path.join(temporaryRoot, "linked-transformer-api");
	const cliProject = path.join(temporaryRoot, "linked-transformer-cli");
	createLinkedTransformerProject(apiProject);
	createLinkedTransformerProject(cliProject);

	const session = createBuildSession({ executable, transformerCallbacks: true });
	try {
		const result = await session.build({ project: apiProject });
		assert.equal(result.ok, true, JSON.stringify(result.diagnostics ?? result.projects?.[0]?.diagnostics));
		assert.match(fs.readFileSync(path.join(apiProject, "out", "main.luau"), "utf8"), /linked:start/);
	} finally {
		await session.dispose();
	}

	execFileSync(executable, ["build", "--project", path.join(cliProject, "tsconfig.json")], {
		cwd: cliProject,
		env: { ...process.env, ROTOR_SIDECAR_PATH: path.join(repoRoot, "tools", "sidecar") },
		stdio: "pipe",
	});
	assert.deepEqual(outputArtifactsForProject(apiProject), outputArtifactsForProject(cliProject));
});

test("concurrent solution projects keep transformer callback state isolated", async () => {
	const firstProject = path.join(temporaryRoot, "transformer-union-first");
	const secondProject = path.join(temporaryRoot, "transformer-union-second");
	createTransformerProject(firstProject, "first", "one");
	createTransformerProject(secondProject, "second", "two");

	const session = createBuildSession({ executable, transformerCallbacks: true });
	let result;
	try {
		result = await session.build({
			roots: [path.join(firstProject, "tsconfig.json"), path.join(secondProject, "tsconfig.json")],
		});
	} finally {
		await session.dispose();
	}

	assert.equal(result.ok, true);
	assert.equal(result.projects.length, 2);
	assert.deepEqual(result.projects.map((project) => project.timings.counts.sidecarSpawns ?? 0), [0, 0]);
	assert.match(fs.readFileSync(path.join(firstProject, "out", "main.luau"), "utf8"), /first:one/);
	assert.match(fs.readFileSync(path.join(secondProject, "out", "main.luau"), "utf8"), /second:two/);
});

test("transformer callback failures belong to their project", async () => {
	const failedProject = path.join(temporaryRoot, "transformer-union-failed");
	const successfulProject = path.join(temporaryRoot, "transformer-union-success");
	createTransformerProject(failedProject, "failed", "one");
	createTransformerProject(successfulProject, "successful", "two");
	fs.writeFileSync(path.join(failedProject, "transformer.js"), 'module.exports = () => { throw new Error("owned callback failure"); };\n');

	const session = createBuildSession({ executable, transformerCallbacks: true });
	let result;
	try {
		result = await session.build({
			roots: [path.join(failedProject, "tsconfig.json"), path.join(successfulProject, "tsconfig.json")],
		});
	} finally {
		await session.dispose();
	}

	assert.equal(result.ok, false);
	assert.deepEqual(result.projects.map((project) => project.status), ["failed", "success"]);
	assert.equal(result.projects[0].diagnostics.length, 1);
	assert.match(result.projects[0].diagnostics[0].message, /owned callback failure/);
	assert.deepEqual(result.projects[1].diagnostics, []);
	assert.match(fs.readFileSync(path.join(successfulProject, "out", "main.luau"), "utf8"), /successful:two/);
});

test("transformer callback diagnostics map back to original source positions", async () => {
	const project = path.join(temporaryRoot, "transformer-diagnostic-map");
	createTransformerProject(project);
	fs.writeFileSync(path.join(project, "src", "main.ts"), 'export const phase: string = "original";\n');
	fs.writeFileSync(
		path.join(project, "transformer.js"),
		`module.exports = (program, config, helpers) => (context) => {
  const ts = helpers.ts;
  const visit = (node) => ts.isStringLiteral(node)
    ? ts.factory.createNumericLiteral(123)
    : ts.visitEachChild(node, visit, context);
  return (sourceFile) => ts.visitNode(sourceFile, visit);
};
`,
	);

	const session = createBuildSession({ executable, transformerCallbacks: true });
	let result;
	try {
		result = await session.build({ project });
	} finally {
		await session.dispose();
	}

	assert.equal(result.ok, false);
	assert.equal(result.projects[0].diagnostics.length, 1);
	assert.equal(result.projects[0].diagnostics[0].code, "TS2322");
	assert.equal(result.projects[0].diagnostics[0].file, "src/main.ts");
	assert.equal(result.projects[0].diagnostics[0].line, 1);
	assert.match(result.projects[0].diagnostics[0].message, /not assignable to type 'string'/);
});

test("one request builds a canonical union with project-owned results and CLI byte parity", async () => {
	const apiRoot = path.join(temporaryRoot, "multi-root-api");
	const cliRoot = path.join(temporaryRoot, "multi-root-cli");
	const fixture = path.join(import.meta.dirname, "fixtures", "multi-root");
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
	assert.deepEqual(result.telemetry, {
		selectedProjects: 3,
		satisfiedProjects: 0,
		scheduledProjects: 3,
		transformedProjects: 3,
		emittedProjects: 3,
	});
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

test("package roots can also be referenced without inheriting another root's context", async () => {
	const root = createRootOverlapFixture("package-root-overlap");
	const configs = ["shared", "consumer", "independent"].map((name) => path.join(root, name, "tsconfig.json"));
	const session = createBuildSession({ executable });
	try {
		for (const roots of [configs, [...configs].reverse()]) {
			const result = await session.build({ roots, selected: configs, satisfied: [] });
			assert.equal(result.ok, true, JSON.stringify(result.diagnostics));
			assert.deepEqual(result.diagnostics, []);
			assert.equal(result.projects.length, 3);
			assert.deepEqual(
				new Set(result.projects.map((project) => project.config)),
				new Set(configs.map((config) => config.split(path.sep).join("/"))),
			);
			assert.ok(result.projects.every((project) => ["success", "no-change"].includes(project.status)));
		}
	} finally {
		await session.dispose();
	}
});

test("package roots import a referenced package through getModule", async () => {
	const root = path.join(temporaryRoot, "package-reference-import");
	const fixture = path.join(import.meta.dirname, "fixtures", "single-project");
	for (const [project, config, manifest] of [
		["lib", { include: ["src/**/*.d.ts"] }, { types: "src/index.d.ts", main: "out/init.luau" }],
		["pkg", { references: [{ path: "../lib" }] }, {}],
	]) {
		const dir = path.join(root, project);
		fs.cpSync(fixture, dir, { recursive: true });
		const configPath = path.join(dir, "tsconfig.json");
		const base = JSON.parse(fs.readFileSync(configPath, "utf8"));
		Object.assign(base.compilerOptions, { composite: true, tsBuildInfoFile: "cache/build.tsbuildinfo" });
		fs.writeFileSync(configPath, JSON.stringify({ ...base, ...config }));
		fs.writeFileSync(path.join(dir, "package.json"), JSON.stringify({ name: `@rbxts/${project}`, ...manifest }));
	}
	fs.rmSync(path.join(root, "lib", "src", "main.ts"));
	fs.writeFileSync(path.join(root, "lib", "src", "index.d.ts"), "export declare const value: number;\n");
	fs.writeFileSync(path.join(root, "lib", "src", "init.luau"), "return { value = 7 }\n");
	fs.writeFileSync(path.join(root, "pkg", "src", "main.ts"), 'import { value } from "@rbxts/lib";\nexport const answer = value;\n');
	fs.mkdirSync(path.join(root, "pkg", "node_modules", "@rbxts"), { recursive: true });
	fs.symlinkSync(path.join(root, "lib"), path.join(root, "pkg", "node_modules", "@rbxts", "lib"), "junction");

	const configs = ["lib", "pkg"].map((name) => path.join(root, name, "tsconfig.json"));
	const session = createBuildSession({ executable });
	try {
		for (const roots of [configs, [...configs].reverse()]) {
			const result = await session.build({ roots, selected: configs, satisfied: [] });
			assert.equal(result.ok, true, JSON.stringify(result.diagnostics));
			assert.deepEqual(result.diagnostics, []);
			assert.match(
				fs.readFileSync(path.join(root, "pkg", "out", "main.luau"), "utf8"),
				/TS\.getModule\(script, "@rbxts", "lib"\)\.out/,
			);
		}
	} finally {
		await session.dispose();
	}
});

test("inferred model roots retain their local context when also referenced", async () => {
	const root = createRootOverlapFixture("model-root-overlap");
	const configs = ["shared", "consumer"].map((name) => path.join(root, name, "tsconfig.json"));
	for (const configPath of configs) {
		const dir = path.dirname(configPath);
		fs.writeFileSync(path.join(dir, "package.json"), JSON.stringify({ name: "model-root-overlap" }));
		const rojoPath = path.join(dir, "default.project.json");
		const rojo = JSON.parse(fs.readFileSync(rojoPath, "utf8"));
		rojo.tree.$className = "Folder";
		fs.writeFileSync(rojoPath, JSON.stringify(rojo));
	}
	const session = createBuildSession({ executable });
	try {
		for (const roots of [configs, [...configs].reverse()]) {
			const result = await session.build({ roots, selected: configs, satisfied: [] });
			assert.equal(result.ok, true, JSON.stringify(result.diagnostics));
			assert.equal(result.projects.length, 2);
			assert.deepEqual(result.diagnostics, []);
		}
	} finally {
		await session.dispose();
	}
});

test("game roots retain their own Rojo and runtime context when also referenced", async () => {
	const root = createRootOverlapFixture("game-root-overlap");
	const configs = ["shared", "consumer"].map((name) => path.join(root, name, "tsconfig.json"));
	for (const configPath of configs) {
		const config = JSON.parse(fs.readFileSync(configPath, "utf8"));
		config.include = ["src"];
		config.rbxts = { type: "game", rojo: "default.project.json" };
		fs.writeFileSync(configPath, JSON.stringify(config));
	}
	const session = createBuildSession({ executable });
	try {
		for (const roots of [configs, [...configs].reverse()]) {
			const result = await session.build({ roots, selected: configs, satisfied: [] });
			assert.equal(result.ok, true, JSON.stringify(result.diagnostics));
			assert.equal(result.projects.length, 2);
			assert.deepEqual(result.diagnostics, []);
		}
		const rojoPath = path.join(root, "consumer", "default.project.json");
		const rojo = JSON.parse(fs.readFileSync(rojoPath, "utf8"));
		rojo.tree.ReplicatedStorage.changed = rojo.tree.ReplicatedStorage.shared;
		delete rojo.tree.ReplicatedStorage.shared;
		fs.writeFileSync(rojoPath, JSON.stringify(rojo));
		const inconsistent = await session.build({ roots: configs, selected: configs, satisfied: [] });
		assert.equal(inconsistent.ok, false);
		assert.match(inconsistent.diagnostics[0].message, /must mount .* at the same Roblox path/);
	} finally {
		await session.dispose();
	}
});

test("package cache state is stable when only the root reaching it changes", async () => {
	const root = createRootOverlapFixture("package-root-cache");
	const shared = path.join(root, "shared", "tsconfig.json");
	const consumer = path.join(root, "consumer", "tsconfig.json");
	const session = createBuildSession({ executable });
	try {
		assert.equal((await session.build({ roots: [shared], selected: [shared], satisfied: [] })).ok, true);
		const manifestPath = path.join(root, "shared", "cache", "build.rbxtsc.tsbuildinfo");
		const before = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
		const declarations = projectArtifacts(root, "shared");
		const result = await session.build({ roots: [consumer], selected: [shared, consumer], satisfied: [] });
		assert.equal(result.ok, true, JSON.stringify(result.diagnostics));
		assert.equal(JSON.parse(fs.readFileSync(manifestPath, "utf8")).salt, before.salt);
		assert.deepEqual(projectArtifacts(root, "shared"), declarations);
	} finally {
		await session.dispose();
	}
});

test("conflicting root contexts return a failure for every selected config", async () => {
	const root = createRootOverlapFixture("conflicting-root-contexts");
	const configs = ["shared", "consumer", "independent"].map((name) => path.join(root, name, "tsconfig.json"));
	for (const configPath of configs.slice(0, 2)) {
		const config = JSON.parse(fs.readFileSync(configPath, "utf8"));
		config.rbxts = { type: "game" };
		fs.writeFileSync(configPath, JSON.stringify(config));
	}
	const session = createBuildSession({ executable });
	try {
		const result = await session.build({ roots: configs, selected: configs, satisfied: [] });
		assert.equal(result.ok, false);
		assert.equal(result.projects.length, 3, JSON.stringify(result));
		assert.deepEqual(
			new Set(result.projects.map((project) => project.config)),
			new Set(configs.map((config) => config.split(path.sep).join("/"))),
		);
		assert.equal(result.diagnostics.length, 1);
		for (const project of result.projects) {
			assert.equal(project.status, "failed");
			assert.equal(project.diagnostics.length, 1);
			assert.match(project.diagnostics[0].message, /conflicting effective options/);
		}
		const message = result.diagnostics[0].message;
		for (const configPath of configs.slice(0, 2)) {
			assert.ok(message.includes(configPath), message);
			assert.ok(message.includes(path.join(path.dirname(configPath), "default.project.json")), message);
		}
		assert.deepEqual(result.telemetry, {
			selectedProjects: 3,
			satisfiedProjects: 0,
			scheduledProjects: 0,
			transformedProjects: 0,
			emittedProjects: 0,
		});
	} finally {
		await session.dispose();
	}
});

test("a union failure owns one diagnostic and explicitly blocks every dependent", async () => {
	const root = path.join(temporaryRoot, "multi-root-failure");
	fs.cpSync(path.join(import.meta.dirname, "fixtures", "multi-root"), root, { recursive: true });
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
	assert.deepEqual(result.telemetry, {
		selectedProjects: 3,
		satisfiedProjects: 0,
		scheduledProjects: 3,
		transformedProjects: 0,
		emittedProjects: 0,
	});
});

test("a mixed cache hit builds selected projects without touching restored dependencies", async () => {
	const fixture = path.join(import.meta.dirname, "fixtures", "multi-root");
	const referenceRoot = path.join(temporaryRoot, "mixed-hit-reference");
	const mixedRoot = path.join(temporaryRoot, "mixed-hit-api");
	fs.cpSync(fixture, referenceRoot, { recursive: true });
	fs.cpSync(fixture, mixedRoot, { recursive: true });

	const buildAll = async (root) => {
		const session = createBuildSession({ executable });
		try {
			return await session.build({
				roots: [path.join(root, "production", "tsconfig.json"), path.join(root, "tests", "tsconfig.json")],
			});
		} finally {
			await session.dispose();
		}
	};
	assert.equal((await buildAll(referenceRoot)).ok, true);
	assert.equal((await buildAll(mixedRoot)).ok, true);
	const restoredBefore = projectArtifacts(mixedRoot, "shared");
	const restoredTimes = Object.fromEntries(
		Object.keys(restoredBefore).map((file) => [file, fs.statSync(path.join(mixedRoot, file)).mtimeMs]),
	);
	fs.rmSync(path.join(mixedRoot, "production", "out"), { force: true, recursive: true });
	fs.rmSync(path.join(mixedRoot, "tests", "out"), { force: true, recursive: true });

	const productionConfig = path.join(mixedRoot, "production", "tsconfig.json");
	const testsConfig = path.join(mixedRoot, "tests", "tsconfig.json");
	const sharedConfig = path.join(mixedRoot, "shared", "tsconfig.json");
	const session = createBuildSession({ executable });
	let result;
	try {
		result = await session.build({
			roots: [productionConfig, testsConfig],
			selected: [productionConfig, testsConfig],
			satisfied: [sharedConfig],
		});
	} finally {
		await session.dispose();
	}

	assert.equal(result.ok, true);
	assert.deepEqual(result.projects.map((project) => project.status), ["satisfied", "success", "success"]);
	assert.deepEqual(result.telemetry, {
		selectedProjects: 2,
		satisfiedProjects: 1,
		scheduledProjects: 2,
		transformedProjects: 2,
		emittedProjects: 2,
	});
	assert.deepEqual(projectArtifacts(mixedRoot, "shared"), restoredBefore);
	assert.deepEqual(
		Object.fromEntries(Object.keys(restoredBefore).map((file) => [file, fs.statSync(path.join(mixedRoot, file)).mtimeMs])),
		restoredTimes,
	);
	for (const project of ["production", "tests"]) {
		assert.deepEqual(projectArtifacts(mixedRoot, project), projectArtifacts(referenceRoot, project));
	}
});

test("missing restored declarations fail the selected project without rebuilding the satisfied dependency", async () => {
	const root = path.join(temporaryRoot, "missing-restored-output");
	fs.cpSync(path.join(import.meta.dirname, "fixtures", "multi-root"), root, { recursive: true });
	const productionConfig = path.join(root, "production", "tsconfig.json");
	const sharedConfig = path.join(root, "shared", "tsconfig.json");
	const session = createBuildSession({ executable });
	try {
		const initial = await session.build({ roots: [productionConfig] });
		assert.equal(initial.ok, true);
		const declaration = path.join(root, "shared", "out", "value.d.ts");
		fs.rmSync(declaration);
		fs.rmSync(path.join(root, "production", "out"), { force: true, recursive: true });

		const result = await session.build({
			roots: [productionConfig],
			selected: [productionConfig],
			satisfied: [sharedConfig],
		});
		assert.equal(result.ok, false);
		assert.deepEqual(result.projects.map((project) => project.status), ["failed", "blocked"]);
		assert.equal(result.projects[0].diagnostics[0].code, "SOLUTION_SATISFIED_OUTPUT_MISSING");
		assert.match(result.projects[0].diagnostics[0].message, /restore the output or select the project for work/);
		assert.deepEqual(result.projects[1].blockers, [result.projects[0].config]);
		assert.equal(fs.existsSync(declaration), false);
		assert.deepEqual(result.telemetry, {
			selectedProjects: 1,
			satisfiedProjects: 1,
			scheduledProjects: 1,
			transformedProjects: 0,
			emittedProjects: 0,
		});
	} finally {
		await session.dispose();
	}
});

test("invalid restored declarations fail without transforming or replacing the satisfied output", async () => {
	const root = path.join(temporaryRoot, "invalid-restored-output");
	fs.cpSync(path.join(import.meta.dirname, "fixtures", "multi-root"), root, { recursive: true });
	const productionConfig = path.join(root, "production", "tsconfig.json");
	const sharedConfig = path.join(root, "shared", "tsconfig.json");
	const session = createBuildSession({ executable });
	try {
		assert.equal((await session.build({ roots: [productionConfig] })).ok, true);
		const declaration = path.join(root, "shared", "out", "value.d.ts");
		const invalidDeclaration = "export declare const sharedValue: ;\n";
		fs.writeFileSync(declaration, invalidDeclaration);
		fs.rmSync(path.join(root, "production", "out"), { force: true, recursive: true });

		const result = await session.build({
			roots: [productionConfig],
			selected: [productionConfig],
			satisfied: [sharedConfig],
		});
		assert.equal(result.ok, false);
		assert.deepEqual(result.projects.map((project) => project.status), ["failed", "blocked"]);
		assert.equal(result.projects[0].diagnostics[0].code, "SOLUTION_SATISFIED_OUTPUT_INVALID");
		assert.match(result.projects[0].diagnostics[0].message, /restore the output or select the project for work/);
		assert.equal(fs.readFileSync(declaration, "utf8"), invalidDeclaration);
		assert.deepEqual(result.telemetry, {
			selectedProjects: 1,
			satisfiedProjects: 1,
			scheduledProjects: 1,
			transformedProjects: 0,
			emittedProjects: 0,
		});
	} finally {
		await session.dispose();
	}
});

test("invalid ownership claims return one owned diagnostic and a terminal result for every selected project", async () => {
	const root = path.join(temporaryRoot, "invalid-ownership");
	fs.cpSync(path.join(import.meta.dirname, "fixtures", "multi-root"), root, { recursive: true });
	const productionConfig = path.join(root, "production", "tsconfig.json");
	const unknownConfig = path.join(root, "unknown", "tsconfig.json");
	const session = createBuildSession({ executable });
	try {
		const result = await session.build({
			roots: [productionConfig],
			selected: [productionConfig],
			satisfied: [unknownConfig],
		});
		assert.equal(result.ok, false);
		assert.equal(result.projects.length, 2);
		assert.equal(result.projects[0].config, productionConfig.split(path.sep).join("/"));
		assert.equal(result.projects[0].status, "failed");
		assert.equal(result.projects[0].diagnostics[0].code, "SOLUTION_SELECTION_MISSING_DEPENDENCY");
		assert.equal(result.projects[1].config, unknownConfig.split(path.sep).join("/"));
		assert.equal(result.projects[1].status, "failed");
		assert.equal(result.projects[1].diagnostics[0].code, "SOLUTION_SELECTION_UNKNOWN");
		assert.match(result.projects[1].diagnostics[0].message, /not part of the discovered solution graph/);
		assert.deepEqual(result.telemetry, {
			selectedProjects: 1,
			satisfiedProjects: 0,
			scheduledProjects: 1,
			transformedProjects: 0,
			emittedProjects: 0,
		});
	} finally {
		await session.dispose();
	}
});

test("an installed package resolves its native executable without an override", async () => {
	const installedPackage = path.join(temporaryRoot, "installed-package");
	fs.mkdirSync(installedPackage, { recursive: true });
	for (const file of ["api.js", "api.d.ts", "package.json", "platform.js"]) {
		fs.copyFileSync(path.join(repoRoot, file), path.join(installedPackage, file));
	}
	fs.mkdirSync(path.join(installedPackage, "tools", "sidecar"), { recursive: true });
	for (const file of ["index.js", "package.json"]) {
		fs.copyFileSync(
			path.join(repoRoot, "tools", "sidecar", file),
			path.join(installedPackage, "tools", "sidecar", file),
		);
	}
	fs.cpSync(path.join(repoRoot, "tools", "sidecar", "lib"), path.join(installedPackage, "tools", "sidecar", "lib"), {
		recursive: true,
	});
	const packageName = `rotor-${process.platform}-${process.arch}`;
	const platformPackage = path.join(installedPackage, "node_modules", "@rotor-rbx", packageName);
	fs.mkdirSync(path.join(platformPackage, "bin"), { recursive: true });
	const extension = process.platform === "win32" ? ".exe" : "";
	fs.writeFileSync(
		path.join(platformPackage, "package.json"),
		`${JSON.stringify({ name: `@rotor-rbx/${packageName}`, version: packageVersion }, null, 2)}\n`,
	);
	const installedExecutable = path.join(platformPackage, "bin", `sloptor${extension}`);
	fs.copyFileSync(executable, installedExecutable);
	if (process.platform !== "win32") fs.chmodSync(installedExecutable, 0o755);

	const installedClient = await import(pathToFileURL(path.join(installedPackage, "api.js")).href);
	const project = path.join(temporaryRoot, "installed-project");
	fs.cpSync(path.join(import.meta.dirname, "fixtures", "single-project"), project, { recursive: true });
	const session = installedClient.createBuildSession();
	try {
		const result = await session.build({ project });
		assert.equal(result.ok, true);
		assert.deepEqual(result.outputs, [path.join(project, "out", "main.luau").split(path.sep).join("/")]);
	} finally {
		await session.dispose();
	}
});

function createRootOverlapFixture(name) {
	const root = path.join(temporaryRoot, name);
	const fixture = path.join(import.meta.dirname, "fixtures", "single-project");
	for (const project of ["shared", "consumer", "independent"]) {
		const dir = path.join(root, project);
		fs.cpSync(fixture, dir, { recursive: true });
		const configPath = path.join(dir, "tsconfig.json");
		const config = JSON.parse(fs.readFileSync(configPath, "utf8"));
		Object.assign(config.compilerOptions, {
			composite: true,
			declaration: true,
			incremental: true,
			tsBuildInfoFile: "cache/build.tsbuildinfo",
		});
		if (project === "shared") config.include = ["src/**/*.d.ts"];
		if (project === "consumer") config.references = [{ path: "../shared" }];
		fs.writeFileSync(configPath, JSON.stringify(config));
		const mounts = { $className: "ReplicatedStorage" };
		for (const mounted of ["shared", "consumer", "independent"]) {
			mounts[mounted] = { $path: `../${mounted}/out` };
			mounts[`${mounted}Include`] = { $path: `../${mounted}/include` };
		}
		fs.writeFileSync(path.join(dir, "default.project.json"), JSON.stringify({
			name: "root-overlap",
			tree: { $className: "DataModel", ReplicatedStorage: mounts },
		}));
	}
	return root;
}

function outputArtifacts(root, projects = ["shared", "production", "tests"]) {
	const artifacts = {};
	for (const project of projects) {
		const output = path.join(root, project, "out");
		for (const entry of fs.readdirSync(output, { recursive: true, withFileTypes: true })) {
			if (!entry.isFile() || entry.name === "rbxts.copyfiles.json") continue;
			const file = path.join(entry.parentPath, entry.name);
			artifacts[path.relative(root, file).split(path.sep).join("/")] = fs.readFileSync(file).toString("base64");
		}
	}
	return artifacts;
}

// Mirrors a pnpm layout: the plugin's realpath sits outside the project, so
// its own `require("typescript")` only resolves through the project.
function createLinkedTransformerProject(project) {
	createTransformerProject(project, "linked");
	const store = `${project}-store`;
	const plugin = path.join(store, "linked-transformer");
	fs.mkdirSync(plugin, { recursive: true });
	fs.renameSync(path.join(project, "transformer.js"), path.join(plugin, "index.js"));
	fs.writeFileSync(path.join(plugin, "package.json"), '{"name":"linked-transformer","main":"index.js"}\n');
	fs.symlinkSync(plugin, path.join(project, "node_modules", "linked-transformer"), "junction");
	const configPath = path.join(project, "tsconfig.json");
	const config = JSON.parse(fs.readFileSync(configPath, "utf8"));
	config.compilerOptions.plugins = [{ transform: "linked-transformer", prefix: "linked" }];
	fs.writeFileSync(configPath, JSON.stringify(config));
}

function createTransformerProject(project, prefix = "callback", phase = "start") {
	fs.cpSync(path.join(import.meta.dirname, "fixtures", "single-project"), project, { recursive: true });
	fs.writeFileSync(path.join(project, "src", "main.ts"), `export const phase = ${JSON.stringify(phase)};\n`);
	fs.writeFileSync(
		path.join(project, "transformer.js"),
		`const ts = require("typescript");
module.exports = (program, config, helpers) => {
  if (helpers.ts !== ts || !program.getTypeChecker()) throw new Error("typescript instance mismatch");
  return (context) => {
    const visit = (node) => ts.isStringLiteral(node)
      ? ts.factory.createStringLiteral(config.prefix + ":" + node.text)
      : ts.visitEachChild(node, visit, context);
    return (sourceFile) => ts.visitNode(sourceFile, visit);
  };
};
`,
	);
	const configPath = path.join(project, "tsconfig.json");
	const config = JSON.parse(fs.readFileSync(configPath, "utf8"));
	config.compilerOptions.plugins = [{ transform: "./transformer.js", prefix }];
	fs.writeFileSync(configPath, JSON.stringify(config));
	const nodeModules = path.join(project, "node_modules");
	fs.mkdirSync(nodeModules, { recursive: true });
	const transformerFixture = path.join(repoRoot, "testdata", "transformers", "project");
	const typescriptPackage = createRequire(import.meta.url).resolve("typescript/package.json", {
		paths: [transformerFixture],
	});
	fs.symlinkSync(path.dirname(typescriptPackage), path.join(nodeModules, "typescript"), "junction");
}

function outputArtifactsForProject(root) {
	const artifacts = {};
	const output = path.join(root, "out");
	for (const entry of fs.readdirSync(output, { recursive: true, withFileTypes: true })) {
		if (!entry.isFile() || entry.name === "rbxts.copyfiles.json") continue;
		const file = path.join(entry.parentPath, entry.name);
		artifacts[path.relative(root, file).split(path.sep).join("/")] = fs.readFileSync(file).toString("base64");
	}
	return artifacts;
}

function projectArtifacts(root, project) {
	return outputArtifacts(root, [project]);
}
