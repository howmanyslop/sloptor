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
	readonly rbxtsgo: ToolStatistics;
	readonly sloptor: ToolStatistics;
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

function getToolStatistics({ name, measurements }: HyperfineResult): ToolStatistics {
	const wall = measurements
		.map((measurement) => measurement.time_wall_clock.value * MILLISECONDS_PER_SECOND)
		.toSorted((left, right) => left - right);

	return {
		name,
		cpu: getMean(measurements.map((measurement) => measurement.time_cpu.value * MILLISECONDS_PER_SECOND)),
		failed: measurements.filter((measurement) => measurement.exit_code !== 0).length,
		instructions: getMean(measurements.map((measurement) => (measurement.instructions?.value ?? 0) / MILLION)),
		median: wall[Math.floor(wall.length / 2)] ?? 0,
		memory: getMean(measurements.map((measurement) => measurement.memory_peak_resident.value / BYTES_PER_MEBIBYTE)),
		runs: wall.length,
		spread: getStandardDeviation(wall),
	};
}

/**
 * Pairs the two tools' results from one hyperfine export.
 *
 * @param project - The fixture that was benchmarked.
 * @param output - The validated hyperfine export.
 * @returns Statistics for both tools.
 */
export function getComparison(project: string, { results }: HyperfineOutput): ProjectComparison {
	const sloptor = results.find((result) => result.name === "sloptor");
	const rbxtsgo = results.find((result) => result.name === "rbxtsgo");
	if (sloptor === undefined || rbxtsgo === undefined) {
		throw new Error(`hyperfine output for ${project} is missing sloptor or rbxtsgo`);
	}

	return { project, rbxtsgo: getToolStatistics(rbxtsgo), sloptor: getToolStatistics(sloptor) };
}
