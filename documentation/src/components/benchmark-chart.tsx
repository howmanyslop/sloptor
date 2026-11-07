import { Predicate } from "effect";
import {
	Bar,
	BarChart,
	CartesianGrid,
	LabelList,
	ReferenceLine,
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
import type { RivalName } from "$data/benchmark-results";

interface BenchmarkChartProperties {
	animate: boolean;
	rows: ReadonlyArray<ChartRow>;
}

interface Series {
	readonly name: RivalName;
	readonly color: string;
	readonly dash: string;
	readonly delay: number;
}

const SERIES: ReadonlyArray<Series> = [
	{ name: "rbxtsgo", color: "var(--data-3)", dash: "0.5 3.5", delay: 0 },
	{ name: "rbxtsc", color: "var(--data-2)", dash: "6 3", delay: 60 },
];

const AXIS_DOMAIN: [number, number] = [1, 32];
const AXIS_TICKS = [1, 2, 4, 8, 16, 32];
const ROW_HEIGHT = 56;
const AXIS_HEIGHT = 56;
const ANIMATION_DURATION = 320;
const MONO = "var(--font-mono)";

const CHART_MARGIN = { bottom: 4, left: 0, right: 48, top: 28 };
const AXIS_TICK = { fill: "var(--ink-3)", fontFamily: MONO, fontSize: 12 };
const BASELINE_LABEL = {
	dy: -22,
	fill: "var(--data-1)",
	fontFamily: MONO,
	fontSize: 12,
	position: "insideTopLeft",
	value: "sloptor",
} as const;
const HOVER_CURSOR = { fill: "var(--marker)", opacity: 0.6 };
const PROJECT_TICK = <BenchmarkProjectTick />;

function formatAxisTick(value: number): string {
	return `${value}×`;
}

function formatBarLabel(value: RenderableText): string {
	return Predicate.isNumber(value) ? `${formatRatio(value)}×` : "";
}

export function BenchmarkChart({ animate, rows }: Readonly<BenchmarkChartProperties>): ReactNode {
	const height = rows.length * ROW_HEIGHT + AXIS_HEIGHT;
	const series = SERIES.map(({ name, color, dash, delay }) => (
		<Bar
			key={name}
			animationBegin={delay}
			animationDuration={ANIMATION_DURATION}
			animationEasing="ease-out"
			barSize={14}
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
				<ReferenceLine
					ifOverflow="visible"
					label={BASELINE_LABEL}
					stroke="var(--data-1)"
					strokeWidth={2}
					x={1}
				/>
				<Tooltip content={BenchmarkTooltip} cursor={HOVER_CURSOR} isAnimationActive={false} />
				{series}
			</BarChart>
		</ResponsiveContainer>
	);
}
