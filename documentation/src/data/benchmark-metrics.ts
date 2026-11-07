import { BENCHMARK_RESULTS, formatMilliseconds, getReport, getRival } from "./benchmark-results";

import type { RivalName, ScenarioName, ToolStatistics } from "./benchmark-results";

export const METRIC_NAMES = ["time", "cpu", "memory", "instructions"] as const;

export type MetricName = (typeof METRIC_NAMES)[number];

export interface Metric {
	readonly format: (value: number) => string;
	readonly label: string;
	readonly noun: string;
	readonly read: (tool: ToolStatistics) => number;
}

export interface SizeLabel {
	readonly meta: string;
	readonly size: string;
}

export interface ToolDetail {
	readonly name: string;
	readonly ratio: number;
	readonly value: string;
}

export interface ChartRow {
	readonly name: string;
	readonly details: ReadonlyArray<ToolDetail>;
	readonly meta: string;
	readonly rbxtsc: number;
	readonly rbxtsgo: number;
	readonly size: string;
}

export interface Average {
	readonly high: number;
	readonly low: number;
	readonly mean: number;
}

export const METRICS: Readonly<Record<MetricName, Metric>> = {
	cpu: {
		format: (value) => `${formatMilliseconds(value)} ms`,
		label: "CPU",
		noun: "CPU time",
		read: (tool) => tool.cpu,
	},
	instructions: {
		format: (value) => `${Math.round(value).toLocaleString("en-US")} M`,
		label: "Instructions",
		noun: "instructions",
		read: (tool) => tool.instructions,
	},
	memory: {
		format: (value) => `${value.toFixed(0)} MB`,
		label: "Memory",
		noun: "peak memory",
		read: (tool) => tool.memory,
	},
	time: {
		format: (value) => `${formatMilliseconds(value)} ms`,
		label: "Time",
		noun: "wall time",
		read: (tool) => tool.median,
	},
};

const METRIC_NAME_LIST: ReadonlyArray<string> = METRIC_NAMES;

export function isMetricName(value: string): value is MetricName {
	return METRIC_NAME_LIST.includes(value);
}

function pluralizeFiles(files: number): string {
	return `${files} ${files === 1 ? "file" : "files"}`;
}

export function getSizeLabel(project: string): SizeLabel {
	const size = BENCHMARK_RESULTS.projects.find((candidate) => candidate.name === project);
	if (size === undefined) {
		return { meta: "", size: "" };
	}

	const lines = size.lines >= 1000 ? `${(size.lines / 1000).toFixed(1)}k` : String(size.lines);
	return { meta: pluralizeFiles(size.files), size: `${pluralizeFiles(size.files)} · ${lines} lines` };
}

export function getRows(scenario: ScenarioName, metric: Metric): Array<ChartRow> {
	return getReport(scenario).comparisons.map((comparison) => {
		const baseline = metric.read(comparison.sloptor);
		const rbxtsgo = metric.read(getRival(comparison, "rbxtsgo"));
		const rbxtsc = metric.read(getRival(comparison, "rbxtsc"));
		const label = getSizeLabel(comparison.project);

		return {
			name: comparison.project,
			details: [
				{ name: "sloptor", ratio: 1, value: metric.format(baseline) },
				{ name: "rbxtsgo", ratio: rbxtsgo / baseline, value: metric.format(rbxtsgo) },
				{ name: "rbxtsc", ratio: rbxtsc / baseline, value: metric.format(rbxtsc) },
			],
			meta: label.meta,
			// The axis starts at 1×, so a tool that beat sloptor would draw
			// nothing.
			rbxtsc: Math.max(1, rbxtsc / baseline),
			rbxtsgo: Math.max(1, rbxtsgo / baseline),
			size: label.size,
		};
	});
}

export function getAverage(scenario: ScenarioName, metric: Metric, rival: RivalName): Average {
	const ratios = getReport(scenario)
		.comparisons.map((comparison) => metric.read(getRival(comparison, rival)) / metric.read(comparison.sloptor))
		.toSorted((left, right) => left - right);

	let logSum = 0;
	for (const ratio of ratios) {
		logSum += Math.log(ratio);
	}

	return {
		high: ratios.at(-1) ?? 1,
		low: ratios[0] ?? 1,
		mean: Math.exp(logSum / ratios.length),
	};
}
