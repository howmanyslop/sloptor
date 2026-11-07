import summary from "./benchmark-results.json";

export type BenchmarkSummary = typeof summary;
export type ScenarioReport = BenchmarkSummary["scenarios"][number];
export type ProjectComparison = ScenarioReport["comparisons"][number];
export type ToolStatistics = ProjectComparison["sloptor"];

export const RIVAL_NAMES = ["rbxtsgo", "rbxtsc"] as const;
export type RivalName = (typeof RIVAL_NAMES)[number];

export const SCENARIO_NAMES = ["cold", "incremental"] as const;
export type ScenarioName = (typeof SCENARIO_NAMES)[number];

export const SCENARIO_LABELS: Readonly<Record<ScenarioName, string>> = {
	cold: "Cold build",
	incremental: "No-op rebuild",
};

const SCENARIO_NAME_LIST: ReadonlyArray<string> = SCENARIO_NAMES;

export function isScenarioName(value: string): value is ScenarioName {
	return SCENARIO_NAME_LIST.includes(value);
}

export const BENCHMARK_RESULTS: BenchmarkSummary = summary;

export function getReport(scenario: ScenarioName): ScenarioReport {
	const report = summary.scenarios.find((candidate) => candidate.scenario === scenario);
	if (report === undefined) throw new Error(`benchmark-results.json has no "${scenario}" scenario.`);
	return report;
}

export function getRival({ project, rivals }: ProjectComparison, name: RivalName): ToolStatistics {
	const rival = rivals.find((candidate) => candidate.name === name);
	if (rival === undefined) throw new Error(`benchmark-results.json has no ${name} result for ${project}.`);
	return rival;
}

export function getToolVersion(name: string): string {
	return summary.tools.find((tool) => tool.name === name)?.version ?? "unknown";
}

export function formatMilliseconds(milliseconds: number): string {
	return milliseconds >= 100 ? milliseconds.toFixed(0) : milliseconds.toFixed(1);
}

export function formatRatio(ratio: number): string {
	return ratio >= 10 ? ratio.toFixed(1) : ratio.toFixed(2);
}
