#!/usr/bin/env bun

import { mkdir, readFile, rm } from "node:fs/promises";
import nodePath from "node:path";
import process from "node:process";
import { EnumType } from "@cliffy/command";
import { type } from "arktype";
import { exec } from "tinyexec";

import { createScriptCommand, parseScriptArgumentsAsync } from "$utilities/command-utilities";

const isMeasurement = type({
	exit_code: "number",
	"instructions?": { value: "number" },
	memory_peak_resident: { value: "number" },
	time_cpu: { value: "number" },
	time_wall_clock: { value: "number" },
}).readonly();

const isHyperfineResult = type({
	name: "string",
	measurements: isMeasurement.array(),
}).readonly();
type HyperfineResult = typeof isHyperfineResult.infer;

const isHyperfineOutput = type({
	results: isHyperfineResult.array().readonly(),
}).readonly();
type HyperfineOutput = typeof isHyperfineOutput.infer;

const enum Scenario {
	Cold = "cold",
	Incremental = "incremental",
}

interface BenchmarkOptions {
	readonly checkers: number;
	readonly out: string;
	readonly rbxtsgo: string;
	readonly runs: number;
	readonly scenario: Scenario;
	readonly sloptor: string;
	readonly warmup: number;
}

const DEFAULT_PROJECTS = ["hello", "algorithms", "containers", "async", "roblox-api", "obby", "wide"];
const BYTES_PER_MEBIBYTE = 1_048_576;
const MILLISECONDS_PER_SECOND = 1000;
const MILLION = 1_000_000;

const repositoryRoot = nodePath.resolve(import.meta.dirname, "..");
const compareDirectory = nodePath.join(repositoryRoot, "benchmarks", "compare");

async function runAsync(command: string, parameters: ReadonlyArray<string>): Promise<string> {
	const result = await exec(command, [...parameters], {
		nodeOptions: { cwd: repositoryRoot },
		throwOnError: true,
	});
	return result.stdout.trim();
}

async function cleanAsync(project: string): Promise<void> {
	await Promise.all(
		["out", "include", ".rotor"].map(async (name) => {
			await rm(nodePath.join(compareDirectory, project, name), { force: true, recursive: true });
		}),
	);
}

function getMean(values: ReadonlyArray<number>): number {
	return values.reduce((sum, value) => sum + value, 0) / values.length;
}

function getStandardDeviation(values: ReadonlyArray<number>): number {
	const average = getMean(values);
	return Math.sqrt(values.reduce((sum, value) => sum + (value - average) ** 2, 0) / (values.length - 1));
}

function summarize(result: HyperfineResult): string {
	const { measurements } = result;
	const wall = measurements
		.map((measurement) => measurement.time_wall_clock.value * MILLISECONDS_PER_SECOND)
		.toSorted((left, right) => left - right);
	const failed = measurements.filter((measurement) => measurement.exit_code !== 0).length;
	const cpu = getMean(measurements.map((measurement) => measurement.time_cpu.value * MILLISECONDS_PER_SECOND));
	const memory = getMean(
		measurements.map((measurement) => measurement.memory_peak_resident.value / BYTES_PER_MEBIBYTE),
	);
	const instructions = getMean(measurements.map((measurement) => (measurement.instructions?.value ?? 0) / MILLION));
	const median = wall[Math.floor(wall.length / 2)] ?? 0;

	return [
		result.name.padEnd(8),
		`n=${wall.length}`,
		`failed=${failed}`,
		`mean ${getMean(wall).toFixed(1)}±${getStandardDeviation(wall).toFixed(1)}ms`,
		`median ${median.toFixed(1)}ms`,
		`min ${(wall[0] ?? 0).toFixed(1)}ms`,
		`cpu ${cpu.toFixed(0)}ms`,
		`rss ${memory.toFixed(0)}MB`,
		`${instructions.toFixed(0)}M instr`,
	].join("  ");
}

async function runBenchmarkAsync(
	project: string,
	{ checkers, out, rbxtsgo, runs, scenario, sloptor, warmup }: BenchmarkOptions,
): Promise<HyperfineOutput> {
	const directory = nodePath.join(compareDirectory, project);
	const output = nodePath.join(out, `${scenario}-${project}.json`);
	const parameters = ["-N", "--warmup", String(warmup), "--runs", String(runs), "--export-json", output];

	if (scenario === Scenario.Cold) {
		parameters.push("--prepare", `rm -rf ${directory}/out ${directory}/include ${directory}/.rotor`);
	}

	parameters.push(
		"-n",
		"sloptor",
		`${sloptor} build -p ${directory} --checkers ${checkers}`,
		"-n",
		"rbxtsgo",
		`${rbxtsgo} -p ${directory} -checkers ${checkers}`,
	);

	await cleanAsync(project);
	await runAsync("hyperfine", parameters);
	await cleanAsync(project);

	const parsed = isHyperfineOutput(JSON.parse(await readFile(output, "utf8")));
	if (parsed instanceof type.errors) throw new Error(`Unexpected hyperfine output in ${output}: ${parsed.summary}`);
	return parsed;
}

const { command, log } = createScriptCommand(
	"1.0.0",
	"Benchmarks sloptor against rbxtsgo on the fixtures in benchmarks/compare. Needs mise (sloptor, rbxtsgo, hyperfine 2.x) and `pnpm install --ignore-workspace` run once inside each fixture.",
);

command
	.type("scenario", new EnumType([Scenario.Cold, Scenario.Incremental]))
	.option("--projects <projects:string[]>", "Fixtures to benchmark.", { default: DEFAULT_PROJECTS })
	.option(
		"--scenarios <scenarios:scenario[]>",
		"Cold builds wipe all outputs first; incremental rebuilds change nothing.",
		{
			default: [Scenario.Cold, Scenario.Incremental],
		},
	)
	.option("--runs <runs:integer>", "Timed runs per command.", { default: 40 })
	.option("--warmup <warmup:integer>", "Untimed warmup runs per command.", { default: 5 })
	.option("--checkers <checkers:integer>", "Checker count handed to both tools.", { default: 4 })
	.option("--out <directory:string>", "Where the raw hyperfine JSON goes.", {
		default: nodePath.join(compareDirectory, "results"),
	})
	.action(async ({ checkers, out, projects, runs, scenarios, warmup }) => {
		try {
			const [sloptor, rbxtsgo] = await Promise.all([
				runAsync("mise", ["which", "sloptor"]),
				runAsync("mise", ["which", "rbxtsgo"]),
				mkdir(out, { recursive: true }),
			]);

			for (const project of projects) {
				for (const scenario of scenarios) {
					// oxlint-disable-next-line no-await-in-loop -- benchmarks must run one at a time or they skew each other
					const { results } = await runBenchmarkAsync(project, {
						checkers,
						out,
						rbxtsgo,
						runs,
						scenario,
						sloptor,
						warmup,
					});

					console.log(`\n== ${project} (${scenario}, checkers=${checkers})`);
					for (const result of results) console.log(summarize(result));
				}
			}
		} catch (error) {
			log.fail(error instanceof Error ? error.message : String(error));
			process.exitCode = 1;
		}
	});

await parseScriptArgumentsAsync(command);
