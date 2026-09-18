#!/usr/bin/env node

import fs from "node:fs";

const STANDARD_METRICS = [
	"clientWallMs",
	"clientUserSeconds",
	"clientSystemSeconds",
	"clientPeakRssBytes",
	"durationMs",
	"timingTotalMs",
	"nodeRequestCpuUserMicroseconds",
	"nodeRequestCpuSystemMicroseconds",
];
const WATCH_METRICS = ["initialReadyElapsedMs", "rebuildElapsedMs"];

function fail(message) {
	throw new Error(message);
}

function parseArguments(argv) {
	const inputs = [];
	for (let index = 0; index < argv.length; index++) {
		if (argv[index] !== "--input") fail(`unexpected argument: ${argv[index]}`);
		const input = argv[++index];
		if (!input) fail("--input requires a report path");
		inputs.push(input);
	}
	if (inputs.length === 0) fail("provide at least one --input report");
	return inputs;
}

function median(values) {
	const sorted = [...values].sort((left, right) => left - right);
	const middle = Math.floor(sorted.length / 2);
	return sorted.length % 2 === 0 ? (sorted[middle - 1] + sorted[middle]) / 2 : sorted[middle];
}

function pairedRecords(records) {
	const pairs = new Map();
	for (const record of records) {
		if (record.label !== "baseline" && record.label !== "candidate")
			fail(`unsupported record label: ${record.label}`);
		const key = `${record.scenario}\u0000${record.repetition}\u0000${record.iteration}`;
		const pair = pairs.get(key) ?? {
			scenario: record.scenario,
			repetition: record.repetition,
			iteration: record.iteration,
		};
		if (pair[record.label]) fail(`duplicate ${record.label} record for ${key}`);
		pair[record.label] = record;
		pairs.set(key, pair);
	}
	for (const pair of pairs.values())
		if (!pair.baseline || !pair.candidate)
			fail(`unpaired record for ${pair.scenario}, repetition ${pair.repetition}, iteration ${pair.iteration}`);
	return [...pairs.values()];
}

function summariseValues(pairs, valueForRecord) {
	const values = pairs.flatMap((pair) => {
		const baseline = valueForRecord(pair.baseline);
		const candidate = valueForRecord(pair.candidate);
		return Number.isFinite(baseline) && Number.isFinite(candidate) ? [{ baseline, candidate }] : [];
	});
	if (values.length === 0) return undefined;
	const baselineMedian = median(values.map((value) => value.baseline));
	const candidateMedian = median(values.map((value) => value.candidate));
	const deltaMedian = candidateMedian - baselineMedian;
	const percentDelta = baselineMedian === 0 ? null : (deltaMedian / baselineMedian) * 100;
	return {
		pairs: values.length,
		baselineMedian,
		candidateMedian,
		deltaMedian,
		percentDelta,
		candidateLower: values.filter((value) => value.candidate < value.baseline).length,
		candidateHigher: values.filter((value) => value.candidate > value.baseline).length,
		tied: values.filter((value) => value.candidate === value.baseline).length,
	};
}

function summariseMetric(pairs, metric) {
	return summariseValues(pairs, (record) => record[metric]);
}

function summariseStages(pairs) {
	const names = new Set(
		pairs.flatMap((pair) => [
			...Object.keys(pair.baseline.stageWorkMs ?? {}),
			...Object.keys(pair.candidate.stageWorkMs ?? {}),
		]),
	);
	return Object.fromEntries(
		[...names].sort().flatMap((name) => {
			const summary = summariseValues(pairs, (record) => record.stageWorkMs?.[name]);
			return summary ? [[name, summary]] : [];
		}),
	);
}

function summariseReport(report) {
	if (report.schemaVersion !== 1 || !Array.isArray(report.records))
		fail("input is not a schemaVersion 1 synthetic worker report");
	const pairs = pairedRecords(report.records);
	const scenarios = {};
	for (const scenario of [...new Set(pairs.map((pair) => pair.scenario))].sort()) {
		const scenarioPairs = pairs.filter((pair) => pair.scenario === scenario);
		const metrics = {};
		for (const metric of scenario === "watch" ? WATCH_METRICS : STANDARD_METRICS) {
			const summary = summariseMetric(scenarioPairs, metric);
			if (summary) metrics[metric] = summary;
		}
		scenarios[scenario] = { pairs: scenarioPairs.length, metrics, stageWorkMs: summariseStages(scenarioPairs) };
	}
	const materialRegressions = Object.entries(scenarios).flatMap(([scenario, summary]) =>
		Object.entries(summary.metrics)
			.filter(([, metric]) => metric.percentDelta !== null && metric.percentDelta > 5)
			.map(([metric, values]) => ({ scenario, metric, ...values })),
	);
	const materialStageRegressions = Object.entries(scenarios).flatMap(([scenario, summary]) =>
		Object.entries(summary.stageWorkMs)
			.filter(([, metric]) => metric.percentDelta !== null && metric.percentDelta > 5)
			.map(([stage, values]) => ({ scenario, stage, ...values })),
	);
	return {
		schemaVersion: report.schemaVersion,
		environment: report.environment,
		fixture: report.fixture,
		binaries: report.binaries,
		records: report.records.length,
		pairedRecords: pairs.length,
		scenarios,
		materialRegressions,
		materialStageRegressions,
	};
}

const inputs = parseArguments(process.argv.slice(2));
const reports = inputs.map((input) => summariseReport(JSON.parse(fs.readFileSync(input, "utf8"))));
process.stdout.write(
	`${JSON.stringify(
		{
			schemaVersion: 1,
			measurementNotes: {
				clientMetrics:
					"client* values are macOS /usr/bin/time measurements of the command client. They account for the process waited on by time, including its direct child work, but not a persistent worker detached from that command.",
				nodeRequestMetrics:
					"nodeRequestCpu* values are request counters when emitted. They omit program-only warm work and deferred control work; missing values are excluded, never converted to zero.",
				stageMetrics:
					"stageWorkMs values are numeric aggregate work milliseconds. They may sum above timingTotalMs because concurrent projects overlap.",
				daemonMetrics:
					"Persistent daemon RSS and instantaneous CPU samples are intentionally not aggregated as request CPU or total RSS.",
				comparison:
					"Every pair uses matching scenario, repetition, and iteration within one source report. Reports are not pooled across binary hashes. Medians are used for all reported central estimates.",
			},
			reports,
		},
		null,
		2,
	)}\n`,
);
