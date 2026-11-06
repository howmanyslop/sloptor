import { Ansis } from "ansis";

import { getGeometricMean } from "./benchmark-statistics-utilities";

import type { Ansis as Paint } from "ansis";

import type {
	ProjectComparison,
	Scenario,
	ScenarioReport,
	ToolStatistics,
	ToolVersion,
} from "./benchmark-statistics-utilities";

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
	readonly own: ToolStatistics;
	readonly peers: ReadonlyArray<ToolStatistics>;
	readonly project: string;
	readonly result: string;
}

interface ToolPair {
	readonly rival: ToolStatistics;
	readonly sloptor: ToolStatistics;
}

const ansi = new Ansis();
const gray = ansi.hex("8b8b8b");
const rule = ansi.hex("55595f");

const TOOL_COLORS: ReadonlyMap<string, Paint> = new Map([
	["rbxtsc", ansi.hex("8fb4ff")],
	["rbxtsgo", ansi.hex("ffb454")],
	["sloptor", ansi.hex("5ee6b0")],
]);

const BAR_WIDTH = 28;
const COLUMN_GAP = "  ";
const TOOL_WIDTH = 7;
// Fits "10.00× slower".
const RESULT_WIDTH = 13;
const SUMMARY_LABEL_WIDTH = "NO-OP REBUILD".length;
const SUMMARY_RIVAL_WIDTH = "vs rbxtsgo".length;
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

function getToolColor(name: string): Paint {
	return TOOL_COLORS.get(name) ?? gray;
}

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
	const tools = comparisons.flatMap(({ rivals, sloptor }) => [sloptor, ...rivals]);

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

/**
 * Lights up the lowest figure in a group and fades the rest back.
 *
 * @param own - This tool's figure.
 * @param all - The same figure for every tool in the group.
 * @param text - The formatted figure to paint.
 * @returns The painted text.
 */
function paintBest(own: number, all: ReadonlyArray<number>, text: string): string {
	return own <= getSmallest(all) * (1 + NOTICEABLE_DIFFERENCE) ? ansi.greenBright(text) : gray(text);
}

function describeVerdict(rival: ToolStatistics, sloptor: ToolStatistics): string {
	const ratio = rival.median / sloptor.median;

	if (ratio >= TIE_BAND) return ansi.greenBright.bold(`${ratio.toFixed(2)}× slower`);
	if (ratio <= 1 / TIE_BAND) return ansi.redBright.bold(`${(1 / ratio).toFixed(2)}× faster`);
	return gray("≈ even");
}

function describeFailures({ failed }: ToolStatistics): string {
	return failed > 0 ? `${ansi.redBright("✗")} ${failed} failed` : "";
}

function renderToolLine(layout: Layout, { color, fraction, own, peers, project, result }: ToolLine): string {
	return [
		ansi.whiteBright.bold(project.padEnd(layout.project)),
		color(own.name.padEnd(TOOL_WIDTH)),
		color.bold(formatMilliseconds(own.median).padStart(layout.median)),
		gray(formatSpread(own.spread).padStart(layout.spread)),
		drawBar(fraction, color),
		paintBest(
			own.cpu,
			peers.map((peer) => peer.cpu),
			formatMilliseconds(own.cpu).padStart(layout.cpu),
		),
		paintBest(
			own.memory,
			peers.map((peer) => peer.memory),
			formatMemory(own.memory).padStart(layout.memory),
		),
		paintBest(
			own.instructions,
			peers.map((peer) => peer.instructions),
			formatInstructions(own.instructions).padStart(layout.instructions),
		),
		result,
	]
		.join(COLUMN_GAP)
		.trimEnd();
}

function renderComparison(layout: Layout, { project, rivals, sloptor }: ProjectComparison): string {
	const tools = [sloptor, ...rivals];
	const slowest = getLargest(tools.map((tool) => tool.median));

	return tools
		.map((own, index) => {
			const verdict = index === 0 ? gray("baseline") : describeVerdict(own, sloptor);

			return renderToolLine(layout, {
				color: getToolColor(own.name),
				fraction: own.median / slowest,
				own,
				peers: tools,
				project: index === 0 ? project : "",
				result: [verdict, describeFailures(own)].filter((text) => text.length > 0).join(COLUMN_GAP),
			});
		})
		.join("\n");
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
		header("VS SLOPTOR"),
	];
	const width = layout.project + TOOL_WIDTH + layout.median + layout.spread + BAR_WIDTH;
	const total = width + layout.cpu + layout.memory + layout.instructions + RESULT_WIDTH + COLUMN_GAP.length * 8;

	return [columns.join(COLUMN_GAP), rule("─".repeat(total))];
}

function describeCheckers(checkers: number, comparisons: ReadonlyArray<ProjectComparison>): string {
	const hasRbxtsc = comparisons.some(({ rivals }) => rivals.some((rival) => rival.name === "rbxtsc"));
	return hasRbxtsc ? `checkers ${checkers} (rbxtsc has no such flag)` : `checkers ${checkers}`;
}

/**
 * Renders one scenario as a head-to-head table: one line per tool for each
 * project, with bars scaled to the slowest.
 *
 * @param report - The measured comparisons.
 * @returns The table, without a trailing newline.
 */
export function renderScenario({ checkers, comparisons, runs, scenario }: ScenarioReport): string {
	const layout = createLayout(comparisons);
	const details = gray(
		`${DESCRIPTIONS[scenario]} · ${describeCheckers(checkers, comparisons)} · median of ${runs} runs`,
	);
	const title = `${ansi.cyanBright.bold(TITLES[scenario])}  ${details}`;
	const body = comparisons.map((comparison) => renderComparison(layout, comparison));

	return [title, "", ...renderHeader(layout), body.join("\n\n")].join("\n");
}

function formatRatio(ratio: number, better: string, worse: string): string {
	if (ratio >= 1) return ansi.greenBright(`${ratio.toFixed(2)}× ${better}`);
	return ansi.redBright(`${(1 / ratio).toFixed(2)}× ${worse}`);
}

function getAverageRatio(pairs: ReadonlyArray<ToolPair>, getValue: (tool: ToolStatistics) => number): number {
	return getGeometricMean(pairs.map(({ rival, sloptor }) => getValue(rival) / getValue(sloptor)));
}

function getPairs(comparisons: ReadonlyArray<ProjectComparison>, name: string): ReadonlyArray<ToolPair> {
	return comparisons.flatMap(({ rivals, sloptor }) => {
		const rival = rivals.find((candidate) => candidate.name === name);
		return rival === undefined ? [] : [{ rival, sloptor }];
	});
}

function renderSummaryLine(scenario: Scenario, name: string, pairs: ReadonlyArray<ToolPair>, isFirst: boolean): string {
	const speedups = pairs.map(({ rival, sloptor }) => rival.median / sloptor.median);
	const range = `(${getSmallest(speedups).toFixed(2)}×–${getLargest(speedups).toFixed(2)}×)`;

	return [
		ansi.cyanBright.bold((isFirst ? TITLES[scenario] : "").padEnd(SUMMARY_LABEL_WIDTH)),
		getToolColor(name)(`vs ${name}`.padEnd(SUMMARY_RIVAL_WIDTH)),
		ansi.bold(formatRatio(getGeometricMean(speedups), "faster", "slower")),
		gray(range),
		formatRatio(
			getAverageRatio(pairs, (tool) => tool.cpu),
			"less CPU",
			"more CPU",
		),
		formatRatio(
			getAverageRatio(pairs, (tool) => tool.memory),
			"less memory",
			"more memory",
		),
		formatRatio(
			getAverageRatio(pairs, (tool) => tool.instructions),
			"fewer instructions",
			"more instructions",
		),
	]
		.join(COLUMN_GAP)
		.trimEnd();
}

function renderSummaryLines({ comparisons, scenario }: ScenarioReport): ReadonlyArray<string> {
	const names = comparisons[0]?.rivals.map((rival) => rival.name) ?? [];
	return names.map((name, index) => renderSummaryLine(scenario, name, getPairs(comparisons, name), index === 0));
}

/**
 * Renders the closing summary: the geometric mean of every ratio, per
 * scenario, with sloptor measured against each rival.
 *
 * @param reports - Every scenario that ran.
 * @returns The summary block, without a trailing newline.
 */
export function renderSummary(reports: ReadonlyArray<ScenarioReport>): string {
	const projects = getLargest(reports.map(({ comparisons }) => comparisons.length));
	const details = gray(`sloptor against each tool · geometric mean over ${projects} projects`);
	const title = `${gray.bold("SUMMARY")}  ${details}`;

	return [title, "", ...reports.flatMap((report) => renderSummaryLines(report))].join("\n");
}

/**
 * Renders the heading naming every tool and its version.
 *
 * @param tools - Each tool and the version that was installed.
 * @returns The heading line.
 */
export function renderTitle(tools: ReadonlyArray<ToolVersion>): string {
	return tools
		.map(({ name, version }) => `${getToolColor(name).bold(name)} ${gray(version)}`)
		.join(` ${gray("vs")} `);
}
