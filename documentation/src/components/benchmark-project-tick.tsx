import { getSizeLabel } from "$data/benchmark-metrics";

import type { ReactNode } from "react";

type NumberOrString = number | string;
interface Payload {
	readonly value?: NumberOrString | undefined;
}

interface ProjectTickProperties {
	payload?: Payload | undefined;
	x?: NumberOrString | undefined;
	y?: NumberOrString | undefined;
}

const LABEL_GAP = 8;

/**
 * A y-axis label: the project's name, with its size in smaller type below.
 *
 * @param properties - The tick position and value that recharts passes in.
 * @returns The label.
 */
export function BenchmarkProjectTick({ payload, x, y }: Readonly<ProjectTickProperties>): ReactNode {
	const name = String(payload?.value ?? "");
	const left = Number(x) - LABEL_GAP;

	return (
		<text textAnchor="end" x={left} y={Number(y)}>
			<tspan dy="-0.1em" fill="var(--ink-2)" fontFamily="var(--font-mono)" fontSize={12} x={left}>
				{name}
			</tspan>
			<tspan dy="1.35em" fill="var(--ink-3)" fontFamily="var(--font-mono)" fontSize={11} x={left}>
				{getSizeLabel(name).meta}
			</tspan>
		</text>
	);
}
