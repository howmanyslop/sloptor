import { spawn, spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

const SCENARIOS = new Set([
	"cold",
	"no-change",
	"one-file-warm",
	"missing-output-warm",
	"declaration-only",
	"source-map",
	"watch",
]);

function fail(message) {
	throw new Error(message);
}

function parseArgs(argv) {
	const result = new Map();
	for (let index = 0; index < argv.length; index++) {
		const flag = argv[index];
		if (flag === "--execute") {
			result.set("execute", "true");
			continue;
		}
		if (!flag.startsWith("--")) fail(`unexpected argument: ${flag}`);
		const value = argv[++index];
		if (value === undefined) fail(`missing value for ${flag}`);
		result.set(flag.slice(2), value);
	}
	return result;
}

function digest(root) {
	const hash = createHash("sha256");
	const projectRoots = [path.resolve(root), fs.realpathSync(root)];
	function mapPath(value) {
		if (typeof value !== "string" || !path.isAbsolute(value)) return value;
		for (const projectRoot of projectRoots) {
			const relative = path.relative(projectRoot, value);
			if (relative !== ".." && !relative.startsWith(`..${path.sep}`) && !path.isAbsolute(relative))
				return `<project>/${relative.split(path.sep).join("/")}`;
		}
		return value;
	}
	let files = 0;
	function visit(relative) {
		const full = path.join(root, relative);
		if (!fs.existsSync(full)) return;
		const stat = fs.lstatSync(full);
		if (stat.isFile() && /\.(?:lua|luau|d\.ts|map)$/.test(relative)) {
			hash.update(relative);
			hash.update("\0");
			if (relative.endsWith(".map")) {
				const map = JSON.parse(fs.readFileSync(full, "utf8"));
				if (Object.hasOwn(map, "file")) map.file = mapPath(map.file);
				if (Object.hasOwn(map, "sourceRoot")) map.sourceRoot = mapPath(map.sourceRoot);
				if (Array.isArray(map.sources)) map.sources = map.sources.map(mapPath);
				hash.update(JSON.stringify(map));
			} else {
				hash.update(fs.readFileSync(full));
			}
			files++;
			return;
		}
		if (!stat.isDirectory()) return;
		for (const entry of fs
			.readdirSync(full, { withFileTypes: true })
			.sort((left, right) => left.name.localeCompare(right.name)))
			visit(path.join(relative, entry.name));
	}
	visit("out");
	return { digest: hash.digest("hex"), files };
}

function textDigest(value) {
	return createHash("sha256").update(value).digest("hex");
}

function parseMacosTime(stderr) {
	const result = { userSeconds: 0, systemSeconds: 0, maximumResidentSetSizeBytes: 0 };
	const seen = { user: false, system: false, rss: false };
	for (const line of stderr.split(/\r?\n/)) {
		const cpu = line.match(/^\s*(user|sys)\s+([\d.]+)\s*$/);
		if (cpu) {
			if (cpu[1] === "user") {
				result.userSeconds = Number(cpu[2]);
				seen.user = true;
			} else {
				result.systemSeconds = Number(cpu[2]);
				seen.system = true;
			}
		}
		const rss = line.match(/^\s*(\d+)\s+maximum resident set size\s*$/);
		if (rss) {
			result.maximumResidentSetSizeBytes = Number(rss[1]);
			seen.rss = true;
		}
	}
	if (!seen.user || !seen.system || !seen.rss || result.maximumResidentSetSizeBytes <= 0)
		fail(`macOS time output omitted required CPU/RSS fields: ${stderr}`);
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
		!fs.lstatSync(destinationLink).isSymbolicLink() ||
		fs.readlinkSync(sourceLink) !== fs.readlinkSync(destinationLink)
	)
		fail("fixture copy did not preserve the TypeScript runtime link");
}

function run(binary, args, environment, timeoutMs = 5_000) {
	const result = spawnSync(binary, args, {
		encoding: "utf8",
		env: environment,
		maxBuffer: 32 * 1024 * 1024,
		timeout: timeoutMs,
	});
	if (result.error) fail(`command failed to start: ${result.error.message}`);
	if (result.status !== 0) fail(`command failed: ${result.stderr.slice(0, 1000)}`);
	return result.stdout;
}

function daemonStatus(binary, environment, timeoutMs = 5_000) {
	const output = run(binary, ["daemon", "status"], environment, timeoutMs).trim();
	if (output === "no sidecar daemons running") return [];
	const daemons = [...output.matchAll(/^sidecar daemon [^:]+: running \(pid (\d+), (\d+) workers\)$/gm)].map(
		(match) => ({ pid: Number(match[1]), workers: Number(match[2]) }),
	);
	if (!daemons.length) fail(`unrecognized daemon status: ${output}`);
	return daemons;
}

async function stopOwnedDaemons(binary, environment) {
	run(binary, ["daemon", "stop"], environment, 5_000);
	const deadline = Date.now() + 5_000;
	let finalStatus = "daemon status did not return a result";
	for (;;) {
		try {
			const daemons = daemonStatus(binary, environment, Math.max(1, Math.min(5_000, deadline - Date.now())));
			if (!daemons.length) return;
			finalStatus = `${daemons.length} daemon(s) still reported live`;
		} catch (error) {
			finalStatus = error instanceof Error ? error.message : String(error);
		}
		if (Date.now() >= deadline) fail(`owned daemon runtime did not stop within 5000ms: ${finalStatus}`);
		await new Promise((resolve) => setTimeout(resolve, 50));
	}
}

function sampleDaemonProcesses(daemons) {
	if (!daemons.length) return [];
	const rows = spawnSync("/bin/ps", ["-axo", "pid=,ppid=,rss=,pcpu=,command="], { encoding: "utf8" });
	if (rows.status !== 0) fail(`ps failed: ${rows.stderr}`);
	const processes = rows.stdout
		.split("\n")
		.map((line) => line.trim().match(/^(\d+)\s+(\d+)\s+(\d+)\s+([\d.]+)\s+(.+)$/))
		.filter(Boolean)
		.map((match) => ({
			pid: Number(match[1]),
			ppid: Number(match[2]),
			rssBytes: Number(match[3]) * 1024,
			cpuPercent: Number(match[4]),
			command: match[5],
		}));
	const daemonPids = new Set(daemons.map((daemon) => daemon.pid));
	const related = new Set(daemonPids);
	for (let changed = true; changed;) {
		changed = false;
		for (const process of processes)
			if (related.has(process.ppid) && !related.has(process.pid)) {
				related.add(process.pid);
				changed = true;
			}
	}
	return processes
		.filter((process) => related.has(process.pid))
		.map((process) => ({
			pid: process.pid,
			ppid: process.ppid,
			liveRssBytes: process.rssBytes,
			instantCpuPercent: process.cpuPercent,
			role: daemonPids.has(process.pid)
				? "daemon"
				: process.command.includes("node")
					? "node-worker"
					: "daemon-child",
		}));
}

function nodeWorkerPids(processes) {
	return processes
		.filter((process) => process.role === "node-worker")
		.map((process) => process.pid)
		.sort((left, right) => left - right);
}

function compilerRun(binary, fixture, environment, scenario, iteration) {
	const timingPath = path.join(fixture, `.worker-timings-${scenario}-${iteration}.json`);
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
	const started = performance.now();
	const timed = spawnSync("/usr/bin/time", ["-lp", binary, ...args], {
		cwd: fixture,
		encoding: "utf8",
		env: environment,
		maxBuffer: 32 * 1024 * 1024,
		timeout: 300_000,
	});
	const clientWallMs = Math.round(performance.now() - started);
	if (timed.error) fail(`compiler process error: ${timed.error.message}`);
	const result = JSON.parse(timed.stdout);
	if (timed.status !== 0 || result.ok !== true) fail(`compiler failed: ${JSON.stringify(result).slice(0, 1000)}`);
	const timings = JSON.parse(fs.readFileSync(timingPath, "utf8"));
	const stageWorkMs = Object.fromEntries(
		Object.entries(timings.stages).map(([name, value]) => {
			if (!Number.isFinite(value)) fail(`timing stage ${name} was not numeric`);
			return [name, value];
		}),
	);
	const outputs = digest(fixture);
	const clientResources = parseMacosTime(timed.stderr);
	return {
		clientWallMs,
		clientUserSeconds: clientResources.userSeconds,
		clientSystemSeconds: clientResources.systemSeconds,
		clientPeakRssBytes: clientResources.maximumResidentSetSizeBytes,
		nodeRequestCpuUserMicroseconds: Object.hasOwn(timings.counts, "nodeCpuUserUs")
			? timings.counts.nodeCpuUserUs
			: null,
		nodeRequestCpuSystemMicroseconds: Object.hasOwn(timings.counts, "nodeCpuSystemUs")
			? timings.counts.nodeCpuSystemUs
			: null,
		timingSchemaVersion: timings.schemaVersion,
		timingTotalMs: timings.totalMs,
		timingStageSemantics: timings.stageSemantics,
		stageWorkMs,
		files: result.files,
		durationMs: result.durationMs,
		outputDigest: outputs.digest,
		outputFiles: outputs.files,
		counts: timings.counts,
	};
}

function setSourceMaps(fixture) {
	const config = JSON.parse(fs.readFileSync(path.join(fixture, "tsconfig.json"), "utf8"));
	config.compilerOptions.sourceMap = true;
	fs.writeFileSync(path.join(fixture, "tsconfig.json"), `${JSON.stringify(config, null, 2)}\n`);
}

function editOneFile(fixture, iteration) {
	const source = path.join(fixture, "src", `unit-${iteration % fixtureMetadata.sourceFiles}.ts`);
	const text = fs.readFileSync(source, "utf8");
	fs.writeFileSync(source, text.replace(/(const seed\d+: number = )\d+/, `$1${1000 + iteration}`));
}

function removeRuntimeOutputs(fixture) {
	const removed = [];
	function visit(directory) {
		for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
			const destination = path.join(directory, entry.name);
			if (entry.isDirectory()) visit(destination);
			else if (entry.isFile() && /\.(?:luau|lua)$/.test(entry.name)) {
				fs.unlinkSync(destination);
				removed.push(destination);
			}
		}
	}
	visit(path.join(fixture, "out"));
	if (!removed.length) fail("prime build did not emit runtime Lua outputs");
	return removed;
}

function factoryConfigCacheCounters(counts) {
	return Object.fromEntries(
		Object.entries(counts).filter(([name]) => /(?:factory|config).*cache|cache.*(?:factory|config)/i.test(name)),
	);
}

function equalOutput(reference, candidate) {
	if (reference.scenario === "watch") {
		for (const key of ["initialOutputDigest", "rebuildOutputDigest", "diagnosticDigest"])
			if (reference[key] !== candidate[key]) fail(`watch mismatch: ${key}`);
		return;
	}
	for (const key of ["files", "outputDigest", "outputFiles"])
		if (reference[key] !== candidate[key])
			fail(`output mismatch for ${candidate.scenario} iteration ${candidate.iteration}: ${key}`);
}

function waitFor(predicate, timeoutMs, description) {
	return new Promise((resolve, reject) => {
		const deadline = Date.now() + timeoutMs;
		const poll = () => {
			try {
				if (predicate()) return resolve();
			} catch (error) {
				return reject(error);
			}
			if (Date.now() >= deadline) return reject(new Error(`timed out waiting for ${description}`));
			setTimeout(poll, 20);
		};
		poll();
	});
}

async function stopWatch(child) {
	if (child.exitCode !== null || child.signalCode !== null) return;
	child.kill("SIGINT");
	await Promise.race([
		new Promise((resolve) => child.once("close", resolve)),
		new Promise((resolve) => setTimeout(resolve, 3000)),
	]);
	if (child.exitCode === null && child.signalCode === null) {
		child.kill("SIGKILL");
		await new Promise((resolve) => child.once("close", resolve));
	}
}

async function watchScenario(binary, fixture, environment, timeoutMs, sampleDaemonResidency) {
	const output = { stdout: "", stderr: "" };
	let exit;
	const child = spawn(
		binary,
		["build", "--watch", "--noInclude", "--no-clear", path.join(fixture, "tsconfig.json")],
		{
			cwd: fixture,
			env: environment,
			stdio: ["ignore", "pipe", "pipe"],
		},
	);
	child.stdout.on("data", (chunk) => {
		output.stdout += chunk;
	});
	child.stderr.on("data", (chunk) => {
		output.stderr += chunk;
	});
	child.once("close", (code, signal) => {
		exit = { code, signal };
	});
	try {
		const started = performance.now();
		await waitFor(
			() => {
				if (exit) fail(`watch exited before initial readiness: ${JSON.stringify(exit)}`);
				return output.stdout.includes("watching for changes") && digest(fixture).files > 0;
			},
			timeoutMs,
			"initial watch readiness and output artifact",
		);
		if (output.stderr.trim()) fail(`watch emitted diagnostics during initial build: ${output.stderr}`);
		const initialOutput = digest(fixture);
		const initialResources = sampleDaemonResidency ? sampleDaemonProcesses(daemonStatus(binary, environment)) : [];
		if (sampleDaemonResidency && !nodeWorkerPids(initialResources).length)
			fail("watch initial build did not leave an inspectable Node worker");
		const changedAt = performance.now();
		editOneFile(fixture, 900);
		await waitFor(
			() => {
				if (exit) fail(`watch exited before rebuild: ${JSON.stringify(exit)}`);
				return output.stdout.includes("rebuild #1") && digest(fixture).digest !== initialOutput.digest;
			},
			timeoutMs,
			"watch rebuild event and changed output artifact",
		);
		if (output.stderr.trim()) fail(`watch emitted diagnostics after source change: ${output.stderr}`);
		const rebuiltOutput = digest(fixture);
		const rebuiltResources = sampleDaemonResidency ? sampleDaemonProcesses(daemonStatus(binary, environment)) : [];
		if (sampleDaemonResidency && !nodeWorkerPids(rebuiltResources).length)
			fail("watch rebuild did not leave an inspectable Node worker");
		return {
			scenario: "watch",
			initialReadyElapsedMs: Math.round(changedAt - started),
			rebuildElapsedMs: Math.round(performance.now() - changedAt),
			initialOutputDigest: initialOutput.digest,
			initialOutputFiles: initialOutput.files,
			rebuildOutputDigest: rebuiltOutput.digest,
			rebuildOutputFiles: rebuiltOutput.files,
			diagnosticDigest: textDigest(output.stderr),
			initialDaemonProcesses: initialResources,
			rebuildDaemonProcesses: rebuiltResources,
			limitations:
				"Elapsed rebuild time includes watcher polling, quiet-period settling, and artifact observation. CPU time is not attributed across the long-lived watch client and daemon.",
		};
	} finally {
		await stopWatch(child);
	}
}

const args = parseArgs(process.argv.slice(2));
if (args.get("execute") !== "true")
	fail("benchmark execution is locked; pass --execute only when the compiler slot is reserved");
if (process.platform !== "darwin") fail("this runner samples macOS ps and /usr/bin/time -lp only");
for (const required of ["template", "baseline", "candidate", "output"])
	if (!args.get(required)) fail(`--${required} is required`);
const template = path.resolve(args.get("template"));
const fixtureMetadata = JSON.parse(fs.readFileSync(path.join(template, "synthetic-worker-fixture.json"), "utf8"));
if (!Number.isInteger(fixtureMetadata.sourceFiles) || fixtureMetadata.sourceFiles < 1)
	fail("template metadata must declare a positive sourceFiles count");
const binaries = { baseline: path.resolve(args.get("baseline")), candidate: path.resolve(args.get("candidate")) };
for (const binary of Object.values(binaries))
	if (!fs.statSync(binary).isFile()) fail(`binary is unavailable: ${binary}`);
const baselineDaemon = args.get("baseline-daemon") ?? "false";
if (baselineDaemon !== "true" && baselineDaemon !== "false") fail("--baseline-daemon must be true or false");
const repetitions = Number(args.get("repetitions") ?? 4);
const warmRepetitions = Number(args.get("warm-repetitions") ?? 5);
const watchTimeoutMs = Number(args.get("watch-timeout-ms") ?? 30_000);
const scenarios = (args.get("scenarios") ?? [...SCENARIOS].join(",")).split(",");
for (const scenario of scenarios) if (!SCENARIOS.has(scenario)) fail(`unsupported scenario: ${scenario}`);
if (!Number.isInteger(repetitions) || repetitions < 2 || repetitions % 2 !== 0)
	fail("--repetitions must be an even positive count so AB and BA blocks are balanced");
if (!Number.isInteger(warmRepetitions) || warmRepetitions < 1) fail("--warm-repetitions must be a positive integer");
if (!Number.isInteger(watchTimeoutMs) || watchTimeoutMs < 1) fail("--watch-timeout-ms must be a positive integer");

const workRoot = fs.mkdtempSync(path.join(os.tmpdir(), "synthetic-worker-bench-"));
const records = [];
const ownedDaemonRuntimes = [];
let primaryFailure;
let reportPath;
let report;
try {
	for (let repetition = 0; repetition < repetitions; repetition++)
		for (const label of repetition % 2 === 0 ? ["baseline", "candidate"] : ["candidate", "baseline"])
			for (const scenario of scenarios) {
				const fixture = path.join(workRoot, `${repetition}-${label}-${scenario}`);
				const daemonRuntime = path.join(fixture, ".worker-benchmark-daemon");
				copyFixture(template, fixture);
				if (scenario === "source-map") setSourceMaps(fixture);
				const candidateDaemon = label === "candidate" || baselineDaemon === "true";
				const environment = {
					...process.env,
					ROTOR_DAEMON_RUNTIME_DIR: daemonRuntime,
					SYNTHETIC_FACTORY_CACHE_PROBE: scenario === "missing-output-warm" ? "1" : "0",
				};
				if (candidateDaemon) {
					ownedDaemonRuntimes.push({ binary: binaries[label], environment });
					await stopOwnedDaemons(binaries[label], environment);
				}
				if (scenario === "watch") {
					let watchFailure;
					try {
						records.push({
							label,
							repetition,
							order: repetition % 2 === 0 ? "AB" : "BA",
							iteration: 0,
							...(await watchScenario(
								binaries[label],
								fixture,
								environment,
								watchTimeoutMs,
								candidateDaemon,
							)),
						});
					} catch (error) {
						watchFailure = error;
						throw error;
					} finally {
						if (candidateDaemon)
							try {
								await stopOwnedDaemons(binaries[label], environment);
							} catch (error) {
								if (!watchFailure) throw error;
							}
					}
					continue;
				}
				const warm =
					scenario === "no-change" || scenario === "one-file-warm" || scenario === "missing-output-warm";
				let expectedDaemonPids = [];
				let expectedNodeWorkerPids = [];
				let prime;
				if (warm) {
					prime = compilerRun(binaries[label], fixture, environment, "cold", "prime");
					if (candidateDaemon) {
						const primedDaemons = daemonStatus(binaries[label], environment);
						expectedDaemonPids = primedDaemons.map((daemon) => daemon.pid);
						if (!expectedDaemonPids.length) fail("warm scenario prime did not start a daemon");
						expectedNodeWorkerPids = nodeWorkerPids(sampleDaemonProcesses(primedDaemons));
						if (!expectedNodeWorkerPids.length)
							fail("warm scenario prime did not leave a Node worker to reuse");
					}
				}
				for (let iteration = 0; iteration < (warm ? warmRepetitions : 1); iteration++) {
					if (scenario === "one-file-warm") editOneFile(fixture, iteration);
					const removedOutputs =
						scenario === "missing-output-warm" ? removeRuntimeOutputs(fixture) : undefined;
					const measured = compilerRun(binaries[label], fixture, environment, scenario, iteration);
					if (removedOutputs?.some((output) => !fs.existsSync(output)))
						fail("missing-output recovery did not restore every runtime output");
					if (scenario === "missing-output-warm") {
						if (measured.outputDigest !== prime.outputDigest || measured.outputFiles !== prime.outputFiles)
							fail("missing-output recovery did not restore the prime output tree");
						if (
							!Number.isFinite(prime.counts.selectedSources) ||
							!Number.isFinite(measured.counts.selectedSources) ||
							measured.counts.selectedSources !== prime.counts.selectedSources
						)
							fail("missing-output recovery did not select the prime source set");
						if (
							candidateDaemon &&
							(!Number.isFinite(measured.counts.sidecarRequestBytes) ||
								measured.counts.sidecarRequestBytes <= 0)
						)
							fail("missing-output recovery did not send a sidecar request");
					}
					const daemons = candidateDaemon ? daemonStatus(binaries[label], environment) : [];
					const daemonPids = daemons.map((daemon) => daemon.pid);
					if (candidateDaemon && warm && JSON.stringify(daemonPids) !== JSON.stringify(expectedDaemonPids))
						fail("warm scenario did not reuse the daemon PID");
					if (candidateDaemon && !daemons.length) fail("transform build did not leave an inspectable daemon");
					const daemonProcesses = sampleDaemonProcesses(daemons);
					const currentNodeWorkerPids = nodeWorkerPids(daemonProcesses);
					if (
						candidateDaemon &&
						warm &&
						JSON.stringify(currentNodeWorkerPids) !== JSON.stringify(expectedNodeWorkerPids)
					)
						fail("warm scenario did not reuse the Node worker PID");
					if (
						candidateDaemon &&
						(scenario === "one-file-warm" || scenario === "missing-output-warm") &&
						Object.hasOwn(measured.counts, "sidecarSpawns") &&
						measured.counts.sidecarSpawns !== 0
					)
						fail("warm selected transform spawned a replacement sidecar worker");
					if (
						scenario === "source-map" &&
						!fs.readdirSync(path.join(fixture, "out")).some((file) => file.endsWith(".map"))
					)
						fail("source-map scenario did not emit a map");
					records.push({
						label,
						repetition,
						order: repetition % 2 === 0 ? "AB" : "BA",
						scenario,
						iteration,
						daemonPids,
						daemonWorkers: daemons.map((daemon) => daemon.workers),
						daemonProcesses,
						nodeWorkerPids: currentNodeWorkerPids,
						primeOutputDigest: prime?.outputDigest,
						primeOutputFiles: prime?.outputFiles,
						primeCounts: prime?.counts,
						factoryConfigCacheCounters: factoryConfigCacheCounters(measured.counts),
						...measured,
					});
				}
				if (candidateDaemon) await stopOwnedDaemons(binaries[label], environment);
			}
	for (const scenario of scenarios)
		for (let repetition = 0; repetition < repetitions; repetition++) {
			const baseline = records.filter(
				(record) =>
					record.label === "baseline" && record.scenario === scenario && record.repetition === repetition,
			);
			const candidate = records.filter(
				(record) =>
					record.label === "candidate" && record.scenario === scenario && record.repetition === repetition,
			);
			for (let index = 0; index < baseline.length; index++) equalOutput(baseline[index], candidate[index]);
		}
	reportPath = path.join(
		path.resolve(args.get("output")),
		`synthetic-worker-${new Date().toISOString().replaceAll(":", "-")}.json`,
	);
	report = {
		schemaVersion: 1,
		environment: {
			platform: process.platform,
			architecture: process.arch,
			logicalCpus: os.cpus().length,
			node: process.versions.node,
		},
		fixture: fixtureMetadata,
		daemonControl: { baselineDaemon: baselineDaemon === "true" },
		binaries: Object.fromEntries(
			Object.entries(binaries).map(([label, binary]) => [
				label,
				{ sha256: createHash("sha256").update(fs.readFileSync(binary)).digest("hex") },
			]),
		),
		outputComparison:
			"Lua and declaration bytes match exactly. Source maps compare every JSON field after replacing only absolute file, sourceRoot, and sources paths contained within each isolated fixture root with <project> paths. Mappings, names, source content, relative paths, and external absolute paths remain unchanged.",
		method: "clientUserSeconds, clientSystemSeconds, and clientPeakRssBytes are macOS time measurements. nodeRequestCpu* values are per-request sidecar timing counters. daemonProcesses liveRssBytes and instantCpuPercent are post-run ps samples, not request CPU.",
		baselineDaemonMode:
			"By default, owned daemon cleanup and reuse checks apply only to the candidate. --baseline-daemon true enables the same controls for the baseline when comparing two protocol-2 binaries.",
		workerResidency:
			"One project deliberately measures reuse and does not test the daemon-wide retained-two-idle-worker eviction policy.",
		noChangeLimit:
			"No-change can select zero source transforms, so Node PID reuse there is residency evidence only. one-file-warm changes Program identity and therefore measures incremental session reuse, not transformer-list cache reuse.",
		factoryCacheScenario:
			"The transformer factory increments a module-local generation marker that changes emitted numeric literals only for missing-output-warm. That scenario deletes every emitted runtime Lua/Luau output after priming while preserving declarations, so recovery selects the same full source set and Program as the prime. Candidate recovery must restore every deleted runtime output, match the prime digest/count and selectedSources, send a sidecar request, retain Node worker PIDs, avoid sidecarSpawns when emitted, and record any factory/config cache counters. A cached factory preserves the prime marker; factory re-creation changes it.",
		watchScenario:
			"Watch readiness and rebuild require both observed watch output and matching disk artifacts. Timeout bounds failure; it is not treated as successful completion. Cancellation sends SIGINT, escalates after three seconds, and then stops only the owned daemon runtime.",
		records,
	};
} catch (error) {
	primaryFailure = error;
} finally {
	const cleanupFailures = [];
	for (const owned of ownedDaemonRuntimes) {
		try {
			await stopOwnedDaemons(owned.binary, owned.environment);
		} catch (error) {
			cleanupFailures.push(error);
		}
	}
	if (cleanupFailures.length)
		primaryFailure = new AggregateError(
			[...(primaryFailure ? [primaryFailure] : []), ...cleanupFailures],
			"Benchmark cleanup failed",
		);
}
if (primaryFailure) {
	const failurePath = path.join(
		path.resolve(args.get("output")),
		`failed-worker-${new Date().toISOString().replaceAll(":", "-")}.json`,
	);
	process.stderr.write(`Failed-run evidence target ${failurePath}; fixtures retained at ${workRoot}\n`);
	fs.mkdirSync(path.dirname(failurePath), { recursive: true });
	fs.writeFileSync(
		failurePath,
		`${JSON.stringify({ state: "failed", error: primaryFailure.message, causes: primaryFailure.errors?.map((error) => error.message), workRoot, node: process.versions.node, records }, null, 2)}\n`,
	);
	throw primaryFailure;
}
try {
	fs.mkdirSync(path.dirname(reportPath), { recursive: true });
	fs.writeFileSync(reportPath, `${JSON.stringify(report, null, 2)}\n`);
} catch (error) {
	process.stderr.write(`Report publication failed; benchmark fixtures retained at ${workRoot}\n`);
	throw error;
}
process.stdout.write(`${JSON.stringify({ reportPath, records: records.length })}\n`);
try {
	fs.rmSync(workRoot, { recursive: true, force: true });
} catch (error) {
	process.stderr.write(`Report saved; temporary fixtures retained at ${workRoot}: ${error.message}\n`);
}
