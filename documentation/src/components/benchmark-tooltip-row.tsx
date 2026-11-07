import { formatRatio } from "$data/benchmark-results";

import type { ReactNode } from "react";

import type { ToolDetail } from "$data/benchmark-metrics";

interface BenchmarkTooltipRowProperties {
	detail: ToolDetail;
}

/**
 * One tool's line in the hover card: its name, its value, and how it compares
 * with sloptor.
 *
 * @param properties - The detail to show.
 * @returns Three cells in a line, ready for the hover card.
 */
export function BenchmarkTooltipRow({ detail }: Readonly<BenchmarkTooltipRowProperties>): ReactNode {
	const { name, ratio, value } = detail;

	return (
		<div className={`bench-tooltip-row bench-tooltip-row--${name}`}>
			<span>{name}</span>
			<span>{value}</span>
			<span>{name === "sloptor" ? "1×" : `${formatRatio(ratio)}×`}</span>
		</div>
	);
}
