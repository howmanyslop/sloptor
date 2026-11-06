#!/usr/bin/env bun

import { mkdir, readFile, rm } from "node:fs/promises";
import nodePath from "node:path";
import process from "node:process";
import { EnumType } from "@cliffy/command";
import { type } from "arktype";
import { exec } from "tinyexec";

import { renderScenario, renderSummary, renderTitle } from "$utilities/benchmark-render-utilities";
import { getComparison, isHyperfineOutput, SCENARIOS } from "$utilities/benchmark-statistics-utilities";
import { createScriptCommand, parseScriptArgumentsAsync } from "$utilities/command-utilities";

import type {
	HyperfineOutput,
	ProjectComparison,
	Scenario,
	ScenarioReport,
} from "$utilities/benchmark-statistics-utilities";

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

async function runBenchmarkAsync(
	project: string,
	{ checkers, out, rbxtsgo, runs, scenario, sloptor, warmup }: BenchmarkOptions,
): Promise<HyperfineOutput> {
	const directory = nodePath.join(compareDirectory, project);
	const output = nodePath.join(out, `${scenario}-${project}.json`);
	const parameters = ["-N", "--warmup", String(warmup), "--runs", String(runs), "--export-json", output];

	if (scenario === "cold") {
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
	"1.1.0",
	"Benchmarks sloptor against rbxtsgo on the fixtures in benchmarks/compare. Needs mise (sloptor, rbxtsgo, hyperfine 2.x) and `pnpm install --ignore-workspace` run once inside each fixture.",
);

command
	.type("scenario", new EnumType(SCENARIOS))
	.option("--projects <projects:string[]>", "Fixtures to benchmark.", { default: DEFAULT_PROJECTS })
	.option(
		"--scenarios <scenarios:scenario[]>",
		"Cold builds wipe all outputs first; incremental rebuilds change nothing.",
		{
			default: [...SCENARIOS],
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
			console.log(
				`${renderTitle({ rbxtsgo: getInstalledVersion(rbxtsgo), sloptor: getInstalledVersion(sloptor) })}\n`,
			);

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
						runs,
						scenario,
						sloptor,
						warmup,
					});
					comparisons.push(getComparison(project, output));
				}

				const report = { checkers, comparisons, runs, scenario };
				reports.push(report);
				console.log(`\n${renderScenario(report)}\n`);
			}

			console.log(renderSummary(reports));
		} catch (error) {
			log.fail(error instanceof Error ? error.message : String(error));
			process.exitCode = 1;
		}
	});

await parseScriptArgumentsAsync(command);
