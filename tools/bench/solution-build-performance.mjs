import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

const SCENARIOS = new Set(["cold", "no-change", "one-file", "declaration-only"]);

function fail(message) {
	throw new Error(message);
}

function parseArgs(argv) {
	const options = new Map();
	for (let index = 0; index < argv.length; index++) {
		const flag = argv[index];
		if (!flag.startsWith("--")) fail(`unexpected argument: ${flag}`);
		const value = argv[++index];
		if (value === undefined) fail(`missing value for ${flag}`);
		options.set(flag.slice(2), value);
	}
	return options;
}

function sha256(file) {
	return createHash("sha256").update(fs.readFileSync(file)).digest("hex");
}

function readRecipe(root) {
	return JSON.parse(fs.readFileSync(path.join(root, "synthetic-fixture.json"), "utf8"));
}

function digestPaths(root, relativePaths, includeFile = () => true) {
	const hash = createHash("sha256");
	let files = 0;
	function visit(relative) {
		const fullPath = path.join(root, relative);
		if (!fs.existsSync(fullPath)) return;
		const stat = fs.lstatSync(fullPath);
		if (stat.isFile() && includeFile(relative)) {
			hash.update(relative);
			hash.update("\0");
			hash.update(fs.readFileSync(fullPath));
			files++;
			return;
		}
		if (!stat.isDirectory()) return;
		for (const entry of fs
			.readdirSync(fullPath, { withFileTypes: true })
			.sort((left, right) => left.name.localeCompare(right.name))) {
			visit(path.join(relative, entry.name));
		}
	}
	for (const relative of relativePaths.sort((left, right) => left.localeCompare(right))) visit(relative);
	return { digest: hash.digest("hex"), files };
}

function artifactDigests(root, recipe) {
	const projects = Array.from({ length: recipe.projects }, (_, index) => `projects/project-${index}`);
	return {
		output: digestPaths(
			root,
			projects.map((project) => `${project}/out`),
			(relative) => /\.(?:lua|luau|d\.ts|map)$/.test(relative),
		),
		runtimeLibrary: digestPaths(
			root,
			projects.map((project) => `${project}/include/RuntimeLib.lua`),
		),
	};
}

function diagnosticDigest(json) {
	const hash = createHash("sha256");
	for (const diagnostic of json.diagnostics ?? []) {
		hash.update(
			JSON.stringify({
				code: diagnostic.code ?? "",
				severity: diagnostic.severity ?? "",
				message: diagnostic.message ?? "",
			}),
		);
		hash.update("\n");
	}
	return hash.digest("hex");
}

function parseMacosTime(stderr) {
	const result = {};
	for (const line of stderr.split(/\r?\n/)) {
		const cpu = line.match(/^\s*(user|sys)\s+([\d.]+)\s*$/);
		if (cpu) {
			if (cpu[1] === "user") result.userSeconds = Number(cpu[2]);
			else result.systemSeconds = Number(cpu[2]);
			continue;
		}
		const rss = line.match(/^\s*(\d+)\s+maximum resident set size\s*$/);
		if (rss) result.maximumResidentSetSizeBytes = Number(rss[1]);
	}
	if (
		!Number.isFinite(result.userSeconds) ||
		!Number.isFinite(result.systemSeconds) ||
		!(result.maximumResidentSetSizeBytes > 0)
	) {
		fail("could not parse CPU time and peak RSS from /usr/bin/time -lp");
	}
	return result;
}

function copyFixture(template, destination) {
	fs.cpSync(template, destination, {
		recursive: true,
		verbatimSymlinks: true,
		filter: (source) => !["out", ".rotor"].includes(path.basename(source)),
	});
	const sourceLink = path.join(template, "node_modules", "typescript");
	const destinationLink = path.join(destination, "node_modules", "typescript");
	if (
		fs.lstatSync(sourceLink).isSymbolicLink() &&
		(!fs.lstatSync(destinationLink).isSymbolicLink() ||
			fs.readlinkSync(sourceLink) !== fs.readlinkSync(destinationLink))
	) {
		fail("fixture copy did not preserve the TypeScript symlink");
	}
}

function command(binary, fixture, timingPath, scenario) {
	const args = [
		"build",
		"--build",
		path.join(fixture, "tsconfig.json"),
		"--noInclude",
		"--json",
		"--timings",
		timingPath,
	];
	if (scenario === "declaration-only") args.push("--emitDeclarationOnly");
	return [binary, ...args];
}

function compactTimings(timings) {
	const metadata = timings.metadata ?? {};
	return {
		schemaVersion: timings.schemaVersion,
		ok: timings.ok,
		totalMs: timings.totalMs,
		stages: timings.stages,
		counts: timings.counts,
		metadata: {
			version: metadata.version,
			goVersion: metadata.goVersion,
			goos: metadata.goos,
			goarch: metadata.goarch,
			gomaxprocs: metadata.gomaxprocs,
			effectiveBuilders: metadata.effectiveBuilders,
			nodeVersion: metadata.nodeVersion,
		},
		projects: (timings.projects ?? []).map((project) => ({
			status: project.status,
			buildWallMs: project.buildWallMs,
			stages: project.stages,
			counts: project.counts,
		})),
	};
}

function execute(binary, fixture, timingPath, scenario) {
	const [executable, ...args] = command(binary, fixture, timingPath, scenario);
	const started = performance.now();
	const run = spawnSync("/usr/bin/time", ["-lp", executable, ...args], {
		cwd: fixture,
		encoding: "utf8",
		maxBuffer: 32 * 1024 * 1024,
		timeout: 300_000,
	});
	if (run.error) fail(`compiler process error for ${scenario}: ${run.error.message}`);
	let json;
	try {
		json = JSON.parse(run.stdout);
	} catch {
		fail(`compiler did not emit one JSON result for ${scenario}`);
	}
	if (run.status !== 0 || json.ok !== true)
		fail(`compiler failed for ${scenario}: status=${run.status} result=${JSON.stringify(json).slice(0, 2000)}`);
	if (!fs.existsSync(timingPath)) fail(`compiler did not write timings for ${scenario}`);
	const timings = JSON.parse(fs.readFileSync(timingPath, "utf8"));
	const artifacts = artifactDigests(fixture, readRecipe(fixture));
	return {
		wallMs: Math.round(performance.now() - started),
		...parseMacosTime(run.stderr),
		reportedDurationMs: json.durationMs,
		files: json.files,
		diagnosticDigest: diagnosticDigest(json),
		outputDigest: artifacts.output.digest,
		outputFiles: artifacts.output.files,
		runtimeLibraryDigest: artifacts.runtimeLibrary.digest,
		runtimeLibraryFiles: artifacts.runtimeLibrary.files,
		timings: compactTimings(timings),
	};
}

function prepareScenario(binary, fixture, recipe, scenario) {
	if (scenario === "cold" || scenario === "declaration-only") return "fresh-fixture";
	execute(binary, fixture, path.join(fixture, ".bench-prime-timings.json"), "cold");
	if (scenario === "one-file") {
		const edited = path.join(fixture, recipe.oneFileEdit);
		fs.writeFileSync(edited, fs.readFileSync(edited, "utf8").replace("value: 0", "value: 1000000"));
	}
	return "primed-in-fresh-fixture";
}

function assertEquivalent(baseline, candidate) {
	for (const key of [
		"outputDigest",
		"outputFiles",
		"runtimeLibraryDigest",
		"runtimeLibraryFiles",
		"diagnosticDigest",
		"files",
	]) {
		if (baseline[key] !== candidate[key]) fail(`correctness mismatch for ${candidate.scenario}: ${key}`);
	}
}

const options = parseArgs(process.argv.slice(2));
if (process.platform !== "darwin")
	fail("This harness uses macOS /usr/bin/time -lp and does not claim Windows or other-platform performance.");
const required = ["template", "baseline", "candidate", "baseline-source", "candidate-source", "output"];
for (const name of required) if (!options.get(name)) fail(`--${name} is required`);
for (const name of ["baseline-source", "candidate-source"])
	if (!/^[0-9A-Za-z][0-9A-Za-z._-]*$/.test(options.get(name)))
		fail(`--${name} must be a source reference, not a path`);

const template = path.resolve(options.get("template"));
const baseline = path.resolve(options.get("baseline"));
const candidate = path.resolve(options.get("candidate"));
const outputDirectory = path.resolve(options.get("output"));
const repetitions = Number(options.get("repetitions") ?? 3);
const scenarios = (options.get("scenarios") ?? [...SCENARIOS].join(",")).split(",");
if (!Number.isInteger(repetitions) || repetitions < 1) fail("--repetitions must be a positive integer");
for (const scenario of scenarios) if (!SCENARIOS.has(scenario)) fail(`unsupported scenario: ${scenario}`);
for (const binary of [baseline, candidate]) if (!fs.statSync(binary).isFile()) fail(`binary is unavailable: ${binary}`);

const recipe = readRecipe(template);
const identities = {
	baseline: { source: options.get("baseline-source"), binarySha256: sha256(baseline) },
	candidate: { source: options.get("candidate-source"), binarySha256: sha256(candidate) },
};
const workRoot = fs.mkdtempSync(path.join(os.tmpdir(), "synthetic-solution-bench-"));
const createdAt = new Date().toISOString();
const records = [];
let ordinal = 0;

try {
	for (let repetition = 0; repetition < repetitions; repetition++) {
		for (const label of repetition % 2 === 0 ? ["baseline", "candidate"] : ["candidate", "baseline"]) {
			for (const scenario of scenarios) {
				const fixture = path.join(workRoot, `${String(ordinal++).padStart(3, "0")}-${label}-${scenario}`);
				copyFixture(template, fixture);
				const projectCacheState = prepareScenario(
					label === "baseline" ? baseline : candidate,
					fixture,
					recipe,
					scenario,
				);
				const measured = execute(
					label === "baseline" ? baseline : candidate,
					fixture,
					path.join(fixture, ".bench-timings.json"),
					scenario,
				);
				records.push({
					repetition,
					order: repetition % 2 === 0 ? "AB" : "BA",
					binary: label,
					sourceIdentity: identities[label].source,
					binarySha256: identities[label].binarySha256,
					scenario,
					projectCacheState,
					osCacheState: "uncontrolled-not-flushed",
					...measured,
				});
			}
		}
	}
	for (const scenario of scenarios) {
		const baselineRecords = records.filter(
			(record) => record.binary === "baseline" && record.scenario === scenario,
		);
		const candidateRecords = records.filter(
			(record) => record.binary === "candidate" && record.scenario === scenario,
		);
		for (let index = 0; index < repetitions; index++)
			assertEquivalent(baselineRecords[index], candidateRecords[index]);
	}
	fs.mkdirSync(outputDirectory, { recursive: true });
	const report = {
		schemaVersion: 1,
		createdAt,
		fixture: recipe,
		fixtureRecipeDigest: fs.readFileSync(path.join(template, "synthetic-fixture.sha256"), "utf8").trim(),
		machine: { platform: process.platform, architecture: process.arch, logicalCpuCount: os.cpus().length },
		method: {
			order: "AB then BA per repetition",
			projectCaches:
				"Each timed invocation has an isolated fixture. no-change and one-file are primed; cold and declaration-only are unprimed.",
			osCaches: "Not flushed or claimed cold.",
			artifactCheck:
				"Emitted Lua, Luau, declaration, and map files are hashed under out; RuntimeLib.lua inputs are hashed separately.",
		},
		records,
	};
	const reportPath = path.join(outputDirectory, `synthetic-solution-${createdAt.replaceAll(":", "-")}.json`);
	fs.writeFileSync(reportPath, `${JSON.stringify(report, null, 2)}\n`);
	process.stdout.write(`${JSON.stringify({ reportPath, records: records.length })}\n`);
} finally {
	fs.rmSync(workRoot, { recursive: true, force: true });
}
