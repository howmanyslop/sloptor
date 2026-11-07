#!/usr/bin/env bun

import { mkdir, readdir, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import nodePath from "node:path";
import process from "node:process";
import { EnumType } from "@cliffy/command";
import { type } from "arktype";
import { exec } from "tinyexec";

import { renderScenario, renderSummary, renderTitle } from "$utilities/benchmark-render-utilities";
import { getComparison, isHyperfineOutput, RIVALS, SCENARIOS } from "$utilities/benchmark-statistics-utilities";
import { createScriptCommand, parseScriptArgumentsAsync } from "$utilities/command-utilities";

import type {
	BenchmarkSummary,
	HyperfineOutput,
	MachineInfo,
	ProjectComparison,
	ProjectSize,
	Rival,
	Scenario,
	ScenarioReport,
} from "$utilities/benchmark-statistics-utilities";

interface BenchmarkOptions {
	readonly checkers: number;
	readonly out: string;
	readonly rbxtsgo: string;
	readonly reuse: boolean;
	readonly rivals: ReadonlyArray<Rival>;
	readonly runs: number;
	readonly scenario: Scenario;
	readonly sloptor: string;
	readonly warmup: number;
}

interface ToolCommandOptions {
	readonly checkers: number;
	readonly directory: string;
	readonly rbxtsgo: string;
	readonly rivals: ReadonlyArray<Rival>;
	readonly sloptor: string;
}

const DEFAULT_PROJECTS = ["hello", "algorithms", "containers", "async", "roblox-api", "obby", "wide"];

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

function getInstalledVersion(binary: string): string {
	return nodePath.basename(nodePath.dirname(binary));
}

function getMachineInfo(): MachineInfo {
	const cpus = os.cpus();
	return {
		cores: cpus.length,
		cpu: cpus[0]?.model ?? "unknown",
		memoryGiB: Math.round(os.totalmem() / 2 ** 30),
	};
}

async function getProjectSizeAsync(name: string): Promise<ProjectSize> {
	const source = nodePath.join(compareDirectory, name, "src");
	const entries = await readdir(source, { recursive: true });
	const files = entries.filter((entry) => entry.endsWith(".ts") && !entry.endsWith(".d.ts"));
	const contents = await Promise.all(files.map(async (file) => readFile(nodePath.join(source, file), "utf8")));
	const lines = contents.reduce((total, text) => total + text.trimEnd().split("\n").length, 0);

	return { name, files: files.length, lines };
}

async function getRbxtscVersionAsync(projects: ReadonlyArray<string>): Promise<string> {
	const [project = DEFAULT_PROJECTS[0] ?? "hello"] = projects;
	return runAsync(nodePath.join(compareDirectory, project, "node_modules", ".bin", "rbxtsc"), ["--version"]);
}

function getRivalCommand(rival: Rival, { checkers, directory, rbxtsgo }: ToolCommandOptions): string {
	switch (rival) {
		// Installed per fixture as a devDependency, like a user's project.
		case "rbxtsc":
			return `${directory}/node_modules/.bin/rbxtsc -p ${directory}`;

		case "rbxtsgo":
			return `${rbxtsgo} -p ${directory} -checkers ${checkers}`;

		default:
			throw new Error(`Unknown rival: ${String(rival satisfies never)}`);
	}
}

function getToolParameters(options: ToolCommandOptions): ReadonlyArray<string> {
	const { checkers, directory, rivals, sloptor } = options;

	return [
		"-n",
		"sloptor",
		`${sloptor} build -p ${directory} --checkers ${checkers}`,
		...rivals.flatMap((rival) => ["-n", rival, getRivalCommand(rival, options)]),
	];
}

async function writeSummaryAsync(path: string, summary: BenchmarkSummary): Promise<void> {
	await mkdir(nodePath.dirname(path), { recursive: true });
	await writeFile(path, `${JSON.stringify(summary, undefined, "\t")}\n`);
}

async function readHyperfineOutputAsync(path: string): Promise<HyperfineOutput> {
	const parsed = isHyperfineOutput(JSON.parse(await readFile(path, "utf8")));
	if (parsed instanceof type.errors) {
		throw new TypeError(`Unexpected hyperfine output in ${path}: ${parsed.summary}`);
	}

	return parsed;
}

async function runBenchmarkAsync(
	project: string,
	{ checkers, out, rbxtsgo, reuse, rivals, runs, scenario, sloptor, warmup }: BenchmarkOptions,
): Promise<HyperfineOutput> {
	const directory = nodePath.join(compareDirectory, project);
	const output = nodePath.join(out, `${scenario}-${project}.json`);
	if (reuse) return readHyperfineOutputAsync(output);

	const prepare =
		scenario === "cold" ? ["--prepare", `rm -rf ${directory}/out ${directory}/include ${directory}/.rotor`] : [];
	const parameters = [
		"-N",
		"--warmup",
		String(warmup),
		"--runs",
		String(runs),
		"--export-json",
		output,
		...prepare,
		...getToolParameters({ checkers, directory, rbxtsgo, rivals, sloptor }),
	];

	await cleanAsync(project);
	await runAsync("hyperfine", parameters);
	await cleanAsync(project);

	return readHyperfineOutputAsync(output);
}

const { command, log } = createScriptCommand(
	"1.2.0",
	"Benchmarks sloptor against rivals on the fixtures in benchmarks/compare. Needs mise (sloptor, rbxtsgo, hyperfine 2.x) and `pnpm install --ignore-workspace` run once inside each fixture, which installs rbxtsc.",
);

command
	.type("scenario", new EnumType(SCENARIOS))
	.type("rival", new EnumType(RIVALS))
	.option("--projects <projects:string[]>", "Fixtures to benchmark.", { default: DEFAULT_PROJECTS })
	.option(
		"--against <rivals:rival[]>",
		"Tools to measure sloptor against. rbxtsc is much slower, so drop it for quick runs.",
		{
			default: [...RIVALS],
		},
	)
	.option(
		"--scenarios <scenarios:scenario[]>",
		"Cold builds wipe all outputs first; incremental rebuilds change nothing.",
		{
			default: [...SCENARIOS],
		},
	)
	.option("--runs <runs:integer>", "Timed runs per command.", { default: 40 })
	.option("--warmup <warmup:integer>", "Untimed warmup runs per command.", { default: 5 })
	.option("--checkers <checkers:integer>", "Checker count handed to sloptor and rbxtsgo. rbxtsc has no such flag.", {
		default: 4,
	})
	.option("--out <directory:string>", "Where the raw hyperfine JSON goes.", {
		default: nodePath.join(compareDirectory, "results"),
	})
	.option("--reuse", "Read the hyperfine JSON already in --out instead of running anything.")
	.option("--summary <file:string>", "Also write the documentation site's data to this file.")
	.action(async ({ against, checkers, out, projects, reuse, runs, scenarios, summary, warmup }) => {
		try {
			const [sloptor, rbxtsgo] = await Promise.all([
				runAsync("mise", ["which", "sloptor"]),
				runAsync("mise", ["which", "rbxtsgo"]),
				mkdir(out, { recursive: true }),
			]);
			const rivalVersions = await Promise.all(
				against.map(async (rival) => ({
					name: rival,
					version: rival === "rbxtsgo" ? getInstalledVersion(rbxtsgo) : await getRbxtscVersionAsync(projects),
				})),
			);

			const tools = [{ name: "sloptor", version: getInstalledVersion(sloptor) }, ...rivalVersions];
			console.log(`${renderTitle(tools)}\n`);

			const reports = new Array<ScenarioReport>();
			for (const scenario of scenarios) {
				const comparisons = new Array<ProjectComparison>();

				for (const project of projects) {
					log.start(`${project} · ${scenario}`);

					// oxlint-disable-next-line no-await-in-loop -- benchmarks must run one at a time or they skew each other
					const output = await runBenchmarkAsync(project, {
						checkers,
						out,
						rbxtsgo,
						reuse: reuse === true,
						rivals: against,
						runs,
						scenario,
						sloptor,
						warmup,
					});

					comparisons.push(getComparison(project, output));
				}

				// A reused export may hold a different run count than the flag.
				const report = { checkers, comparisons, runs: comparisons[0]?.sloptor.runs ?? runs, scenario };
				reports.push(report);
				console.log(`\n${renderScenario(report)}\n`);
			}

			console.log(renderSummary(reports));

			if (summary !== undefined) {
				await writeSummaryAsync(summary, {
					checkers,
					hyperfine: await runAsync("hyperfine", ["--version"]),
					machine: getMachineInfo(),
					projects: await Promise.all(projects.map(async (project) => getProjectSizeAsync(project))),
					scenarios: reports,
					tools,
					warmup,
				});
				log.success(`Wrote ${summary}`);
			}
		} catch (error) {
			log.fail(error instanceof Error ? error.message : String(error));
			process.exitCode = 1;
		}
	});

await parseScriptArgumentsAsync(command);
