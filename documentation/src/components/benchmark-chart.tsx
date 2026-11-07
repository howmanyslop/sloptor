import { Predicate } from "effect";
import {
	Bar,
	BarChart,
	CartesianGrid,
	LabelList,
	ResponsiveContainer,
	Tooltip,
	XAxis,
	YAxis,
} from "recharts";

import { formatRatio } from "$data/benchmark-results";

import { BenchmarkProjectTick } from "./benchmark-project-tick";
import { BenchmarkTooltip } from "./benchmark-tooltip";

import type { ReactNode } from "react";
import type { RenderableText } from "recharts";

import type { ChartRow } from "$data/benchmark-metrics";

interface BenchmarkChartProperties {
	animate: boolean;
	rows: ReadonlyArray<ChartRow>;
}

interface Series {
	readonly name: "rbxtsc" | "rbxtsgo" | "sloptor";
	readonly color: string;
	readonly dash: string | undefined;
	readonly delay: number;
}

const SERIES: ReadonlyArray<Series> = [
	{ name: "sloptor", color: "var(--data-1)", dash: undefined, delay: 0 },
	{ name: "rbxtsgo", color: "var(--data-3)", dash: "0.5 3.5", delay: 60 },
	{ name: "rbxtsc", color: "var(--data-2)", dash: "6 3", delay: 120 },
];

// A log axis has no zero, so every bar starts half a step below sloptor's 1×.
// That gives sloptor a bar of its own instead of a line.
const AXIS_DOMAIN: [number, number] = [0.5, 32];
const AXIS_TICKS = [0.5, 1, 2, 4, 8, 16, 32];
const ROW_HEIGHT = 64;
const AXIS_HEIGHT = 56;
const ANIMATION_DURATION = 320;
const MONO = "var(--font-mono)";

const CHART_MARGIN = { bottom: 4, left: 0, right: 48, top: 8 };
const AXIS_TICK = { fill: "var(--ink-3)", fontFamily: MONO, fontSize: 12 };
const HOVER_CURSOR = { fill: "var(--marker)", opacity: 0.6 };
const PROJECT_TICK = <BenchmarkProjectTick />;

function formatAxisTick(value: number): string {
	return `${value}×`;
}

function formatBarLabel(value: RenderableText): string {
	if (!Predicate.isNumber(value)) return "";
	return value === 1 ? "1×" : `${formatRatio(value)}×`;
}


export function BenchmarkChart({ animate, rows }: Readonly<BenchmarkChartProperties>): ReactNode {
	const height = rows.length * ROW_HEIGHT + AXIS_HEIGHT;
	const series = SERIES.map(({ name, color, dash, delay }) => (
		<Bar
			key={name}
			animationBegin={delay}
			animationDuration={ANIMATION_DURATION}
			animationEasing="ease-out"
			barSize={12}
			dataKey={name}
			fill={color}
			fillOpacity={0.16}
			isAnimationActive={animate}
			stroke={color}
			strokeDasharray={dash}
			strokeLinecap="round"
			strokeWidth={1.5}
		>
			<LabelList
				dataKey={name}
				fill="var(--ink-2)"
				fontFamily={MONO}
				fontSize={12}
				formatter={formatBarLabel}
				position="right"
			/>
		</Bar>
	));

	return (
		<ResponsiveContainer height={height} initialDimension={{ height, width: 720 }} width="100%">
			<BarChart
				accessibilityLayer={false}
				barCategoryGap="26%"
				barGap={3}
				data={rows}
				layout="vertical"
				margin={CHART_MARGIN}
			>
				<CartesianGrid horizontal={false} stroke="var(--rule)" />
				<XAxis
					axisLine={false}
					domain={AXIS_DOMAIN}
					scale="log"
					tick={AXIS_TICK}
					tickFormatter={formatAxisTick}
					tickLine={false}
					ticks={AXIS_TICKS}
					type="number"
				/>
				<YAxis
					axisLine={false}
					dataKey="name"
					tick={PROJECT_TICK}
					tickLine={false}
					type="category"
					width={112}
				/>
				<Tooltip content={BenchmarkTooltip} cursor={HOVER_CURSOR} isAnimationActive={false} />
				{series}
			</BarChart>
		</ResponsiveContainer>
	);
}
