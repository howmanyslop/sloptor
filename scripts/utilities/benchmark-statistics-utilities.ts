import { type } from "arktype";

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

export const isHyperfineOutput = type({
	results: isHyperfineResult.array().readonly(),
}).readonly();

export type HyperfineOutput = typeof isHyperfineOutput.infer;

export const SCENARIOS = ["cold", "incremental"] as const;

export type Scenario = (typeof SCENARIOS)[number];

/** The tools sloptor is measured against. */
export const RIVALS = ["rbxtsgo", "rbxtsc"] as const;

export type Rival = (typeof RIVALS)[number];

export interface ToolStatistics {
	readonly name: string;
	/** Mean CPU time in milliseconds. */
	readonly cpu: number;
	/** Runs that exited non-zero. */
	readonly failed: number;
	/** Mean instructions retired, in millions. */
	readonly instructions: number;
	/** Median wall-clock time in milliseconds. */
	readonly median: number;
	/** Mean peak resident memory in mebibytes. */
	readonly memory: number;
	readonly runs: number;
	/** Wall-clock standard deviation in milliseconds. */
	readonly spread: number;
}

export interface ProjectComparison {
	readonly project: string;
	readonly rivals: ReadonlyArray<ToolStatistics>;
	readonly sloptor: ToolStatistics;
}

export interface ToolVersion {
	readonly name: string;
	readonly version: string;
}

export interface MachineInfo {
	readonly cores: number;
	readonly cpu: string;
	readonly memoryGiB: number;
}

export interface ProjectSize {
	readonly name: string;
	/** TypeScript source files, declaration files excluded. */
	readonly files: number;
	readonly lines: number;
}

export interface BenchmarkSummary {
	readonly checkers: number;
	readonly hyperfine: string;
	readonly machine: MachineInfo;
	readonly projects: ReadonlyArray<ProjectSize>;
	readonly scenarios: ReadonlyArray<ScenarioReport>;
	readonly tools: ReadonlyArray<ToolVersion>;
	readonly warmup: number;
}

export interface ScenarioReport {
	readonly checkers: number;
	readonly comparisons: ReadonlyArray<ProjectComparison>;
	readonly runs: number;
	readonly scenario: Scenario;
}

const BYTES_PER_MEBIBYTE = 1_048_576;
const MILLISECONDS_PER_SECOND = 1000;
const MILLION = 1_000_000;

export function getMean(values: ReadonlyArray<number>): number {
	return values.reduce((sum, value) => sum + value, 0) / values.length;
}

export function getGeometricMean(values: ReadonlyArray<number>): number {
	return Math.exp(getMean(values.map((value) => Math.log(value))));
}

function getStandardDeviation(values: ReadonlyArray<number>): number {
	if (values.length < 2) return 0;

	const average = getMean(values);
	return Math.sqrt(values.reduce((sum, value) => sum + (value - average) ** 2, 0) / (values.length - 1));
}

/**
 * Rounds to three decimals, finer than the measurements and easy on the JSON.
 *
 * @param value - The figure to round.
 * @returns The rounded figure.
 */
function roundFigure(value: number): number {
	return Math.round(value * 1000) / 1000;
}

function getToolStatistics({ name, measurements }: HyperfineResult): ToolStatistics {
	const wall = measurements
		.map((measurement) => measurement.time_wall_clock.value * MILLISECONDS_PER_SECOND)
		.toSorted((left, right) => left - right);

	return {
		name,
		cpu: roundFigure(
			getMean(measurements.map((measurement) => measurement.time_cpu.value * MILLISECONDS_PER_SECOND)),
		),
		failed: measurements.filter((measurement) => measurement.exit_code !== 0).length,
		instructions: roundFigure(
			getMean(measurements.map((measurement) => (measurement.instructions?.value ?? 0) / MILLION)),
		),
		median: roundFigure(wall[Math.floor(wall.length / 2)] ?? 0),
		memory: roundFigure(
			getMean(measurements.map((measurement) => measurement.memory_peak_resident.value / BYTES_PER_MEBIBYTE)),
		),
		runs: wall.length,
		spread: roundFigure(getStandardDeviation(wall)),
	};
}

export function getComparison(project: string, { results }: HyperfineOutput): ProjectComparison {
	const sloptor = results.find((result) => result.name === "sloptor");
	if (sloptor === undefined) {
		throw new Error(`hyperfine output for ${project} is missing sloptor`);
	}

	return {
		project,
		rivals: results.flatMap((result) => (result.name === "sloptor" ? [] : [getToolStatistics(result)])),
		sloptor: getToolStatistics(sloptor),
	};
}
