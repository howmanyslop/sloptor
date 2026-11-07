import { BenchmarkTooltipRow } from "./benchmark-tooltip-row";

import type { ReactNode } from "react";
import type { TooltipContentProps } from "recharts";

import type { ChartRow } from "$data/benchmark-metrics";

function isChartRow(value: unknown): value is ChartRow {
	return typeof value === "object" && value !== null && "details" in value && "name" in value;
}

/**
 * The hover card for one project: every tool's value and its ratio to
 * sloptor. It has no motion, because it follows the pointer dozens of times.
 *
 * @param properties - What recharts passes to a custom tooltip.
 * @returns The card, or nothing when no row is hovered.
 */
export function BenchmarkTooltip({ active, payload }: Readonly<TooltipContentProps>): ReactNode {
	const row: unknown = payload[0]?.payload;
	if (!active || !isChartRow(row)) return undefined;

	const details = new Array<ReactNode>();
	for (const detail of row.details) details.push(<BenchmarkTooltipRow key={detail.name} detail={detail} />);

	return (
		<div className="bench-tooltip">
			<p className="bench-tooltip-title">
				{row.name}
				<span>{row.size}</span>
			</p>
			{details}
		</div>
	);
}
