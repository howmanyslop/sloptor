import { Ansis } from "ansis";

import { getGeometricMean } from "./benchmark-statistics-utilities";

import type { Ansis as Paint } from "ansis";

import type { ProjectComparison, Scenario, ScenarioReport, ToolStatistics } from "./benchmark-statistics-utilities";

interface Layout {
	readonly cpu: number;
	readonly instructions: number;
	readonly median: number;
	readonly memory: number;
	readonly project: number;
	readonly spread: number;
}

interface ToolLine {
	readonly color: Paint;
	readonly fraction: number;
	readonly other: ToolStatistics;
	readonly own: ToolStatistics;
	readonly project: string;
	readonly result: string;
}

// Colour is detected from the terminal, FORCE_COLOR and NO_COLOR.
const ansi = new Ansis();
const gray = ansi.hex("8b8b8b");
const rule = ansi.hex("55595f");
const sloptorColor = ansi.hex("5ee6b0");
const rbxtsgoColor = ansi.hex("ffb454");

const BAR_WIDTH = 28;
const COLUMN_GAP = "  ";
const TOOL_WIDTH = 7;
// Fits "10.00× faster".
const RESULT_WIDTH = 13;
const SUMMARY_LABEL_WIDTH = "NO-OP REBUILD".length;
const TIE_BAND = 1.05;
const NOTICEABLE_DIFFERENCE = 0.01;

const TITLES: Record<Scenario, string> = {
	cold: "COLD BUILD",
	incremental: "NO-OP REBUILD",
};

const DESCRIPTIONS: Record<Scenario, string> = {
	cold: "outputs, include/ and .rotor wiped before every run",
	incremental: "nothing changed since the previous build",
};

function formatMilliseconds(value: number): string {
	return `${value >= 100 ? value.toFixed(0) : value.toFixed(1)} ms`;
}

function formatSpread(value: number): string {
	return `±${value >= 100 ? value.toFixed(0) : value.toFixed(1)}`;
}

function formatMemory(value: number): string {
	return `${value.toFixed(0)} MB`;
}

function formatInstructions(value: number): string {
	return `${Math.round(value).toLocaleString("en-US")} M`;
}

function getLargest(values: ReadonlyArray<number>): number {
	let largest = Number.NEGATIVE_INFINITY;
	for (const value of values) largest = Math.max(largest, value);
	return largest;
}

function getSmallest(values: ReadonlyArray<number>): number {
	let smallest = Number.POSITIVE_INFINITY;
	for (const value of values) smallest = Math.min(smallest, value);
	return smallest;
}

function getWidest(header: string, cells: ReadonlyArray<string>): number {
	return getLargest([header.length, ...cells.map((cell) => cell.length)]);
}

function createLayout(comparisons: ReadonlyArray<ProjectComparison>): Layout {
	const tools = comparisons.flatMap(({ rbxtsgo, sloptor }) => [rbxtsgo, sloptor]);

	return {
		cpu: getWidest(
			"CPU",
			tools.map((tool) => formatMilliseconds(tool.cpu)),
		),
		instructions: getWidest(
			"INSTRUCTIONS",
			tools.map((tool) => formatInstructions(tool.instructions)),
		),
		median: getWidest(
			"MEDIAN",
			tools.map((tool) => formatMilliseconds(tool.median)),
		),
		memory: getWidest(
			"MEMORY",
			tools.map((tool) => formatMemory(tool.memory)),
		),
		project: getWidest(
			"PROJECT",
			comparisons.map(({ project }) => project),
		),
		spread: getWidest(
			"SPREAD",
			tools.map((tool) => formatSpread(tool.spread)),
		),
	};
}

function drawBar(fraction: number, paint: Paint): string {
	const halves = fraction > 0 ? Math.max(1, Math.round(fraction * BAR_WIDTH * 2)) : 0;
	const full = Math.floor(halves / 2);
	const hasHalf = halves % 2 === 1;
	const used = full + (hasHalf ? 1 : 0);

	return paint("━".repeat(full) + (hasHalf ? "╸" : "")) + rule("─".repeat(BAR_WIDTH - used));
}

function paintBetter(own: number, other: number, text: string): string {
	if (own < other * (1 - NOTICEABLE_DIFFERENCE)) return ansi.greenBright(text);
	if (own > other * (1 + NOTICEABLE_DIFFERENCE)) return gray(text);
	return text;
}

function describeSpeedup({ rbxtsgo, sloptor }: ProjectComparison): string {
	const ratio = rbxtsgo.median / sloptor.median;
	const failures = rbxtsgo.failed + sloptor.failed;
	const failure = failures > 0 ? `  ${ansi.redBright("✗")} ${failures} failed` : "";

	if (ratio >= TIE_BAND) return ansi.greenBright.bold(`${ratio.toFixed(2)}× faster`) + failure;
	if (ratio <= 1 / TIE_BAND) return ansi.redBright.bold(`${(1 / ratio).toFixed(2)}× slower`) + failure;
	return gray("≈ even") + failure;
}

function renderToolLine(layout: Layout, { color, fraction, other, own, project, result }: ToolLine): string {
	return [
		ansi.whiteBright.bold(project.padEnd(layout.project)),
		color(own.name.padEnd(TOOL_WIDTH)),
		color.bold(formatMilliseconds(own.median).padStart(layout.median)),
		gray(formatSpread(own.spread).padStart(layout.spread)),
		drawBar(fraction, color),
		paintBetter(own.cpu, other.cpu, formatMilliseconds(own.cpu).padStart(layout.cpu)),
		paintBetter(own.memory, other.memory, formatMemory(own.memory).padStart(layout.memory)),
		paintBetter(
			own.instructions,
			other.instructions,
			formatInstructions(own.instructions).padStart(layout.instructions),
		),
		result,
	]
		.join(COLUMN_GAP)
		.trimEnd();
}

function renderComparison(layout: Layout, comparison: ProjectComparison): ReadonlyArray<string> {
	const { project, rbxtsgo, sloptor } = comparison;
	const slower = Math.max(rbxtsgo.median, sloptor.median);

	return [
		renderToolLine(layout, {
			color: sloptorColor,
			fraction: sloptor.median / slower,
			other: rbxtsgo,
			own: sloptor,
			project,
			result: describeSpeedup(comparison),
		}),
		renderToolLine(layout, {
			color: rbxtsgoColor,
			fraction: rbxtsgo.median / slower,
			other: sloptor,
			own: rbxtsgo,
			project: "",
			result: "",
		}),
	];
}

function renderHeader(layout: Layout): ReadonlyArray<string> {
	const header = gray.bold;
	const columns = [
		header("PROJECT".padEnd(layout.project)),
		header("TOOL".padEnd(TOOL_WIDTH)),
		header("MEDIAN".padStart(layout.median)),
		header("SPREAD".padStart(layout.spread)),
		header("TIME".padEnd(BAR_WIDTH)),
		header("CPU".padStart(layout.cpu)),
		header("MEMORY".padStart(layout.memory)),
		header("INSTRUCTIONS".padStart(layout.instructions)),
		header("RESULT"),
	];
	const width = layout.project + TOOL_WIDTH + layout.median + layout.spread + BAR_WIDTH;
	const total = width + layout.cpu + layout.memory + layout.instructions + RESULT_WIDTH + COLUMN_GAP.length * 8;

	return [columns.join(COLUMN_GAP), rule("─".repeat(total))];
}

export function renderScenario({ checkers, comparisons, runs, scenario }: ScenarioReport): string {
	const layout = createLayout(comparisons);
	const details = gray(`${DESCRIPTIONS[scenario]} · checkers ${checkers} · median of ${runs} runs`);
	const title = `${ansi.cyanBright.bold(TITLES[scenario])}  ${details}`;
	const body = comparisons.map((comparison) => renderComparison(layout, comparison).join("\n"));

	return [title, "", ...renderHeader(layout), body.join("\n\n")].join("\n");
}

function formatRatio(ratio: number, better: string, worse: string): string {
	if (ratio >= 1) return ansi.greenBright(`${ratio.toFixed(2)}× ${better}`);
	return ansi.redBright(`${(1 / ratio).toFixed(2)}× ${worse}`);
}

function getAverageRatio(
	comparisons: ReadonlyArray<ProjectComparison>,
	getValue: (tool: ToolStatistics) => number,
): number {
	return getGeometricMean(comparisons.map(({ rbxtsgo, sloptor }) => getValue(rbxtsgo) / getValue(sloptor)));
}

function renderSummaryLine({ comparisons, scenario }: ScenarioReport): string {
	const speedups = comparisons.map(({ rbxtsgo, sloptor }) => rbxtsgo.median / sloptor.median);
	const range = `(${getSmallest(speedups).toFixed(2)}×–${getLargest(speedups).toFixed(2)}×)`;

	return [
		ansi.cyanBright.bold(TITLES[scenario].padEnd(SUMMARY_LABEL_WIDTH)),
		ansi.bold(formatRatio(getGeometricMean(speedups), "faster", "slower")),
		gray(range),
		formatRatio(
			getAverageRatio(comparisons, (tool) => tool.cpu),
			"less CPU",
			"more CPU",
		),
		formatRatio(
			getAverageRatio(comparisons, (tool) => tool.memory),
			"less memory",
			"more memory",
		),
		formatRatio(
			getAverageRatio(comparisons, (tool) => tool.instructions),
			"fewer instructions",
			"more instructions",
		),
	].join(COLUMN_GAP);
}

export function renderSummary(reports: ReadonlyArray<ScenarioReport>): string {
	const projects = getLargest(reports.map(({ comparisons }) => comparisons.length));
	const details = gray(`sloptor vs rbxtsgo · geometric mean over ${projects} projects`);
	const title = `${gray.bold("SUMMARY")}  ${details}`;

	return [title, "", ...reports.map((report) => renderSummaryLine(report))].join("\n");
}

export function renderTitle({ rbxtsgo, sloptor }: { readonly rbxtsgo: string; readonly sloptor: string }): string {
	return [sloptorColor.bold("sloptor"), gray(sloptor), gray("vs"), rbxtsgoColor.bold("rbxtsgo"), gray(rbxtsgo)].join(
		" ",
	);
}
