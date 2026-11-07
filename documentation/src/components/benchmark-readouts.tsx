import { RIVAL_NAMES } from "$data/benchmark-results";

import { BenchmarkReadout } from "./benchmark-readout";

import type { ReactNode } from "react";

import type { Metric } from "$data/benchmark-metrics";
import type { ScenarioName } from "$data/benchmark-results";

interface BenchmarkReadoutsProperties {
	metric: Metric;
	scenario: ScenarioName;
}

/**
 * The readouts column: each rival's headline ratio for the current selection.
 *
 * @param properties - The scenario and measurement being shown.
 * @returns The column.
 */
export function BenchmarkReadouts({ metric, scenario }: Readonly<BenchmarkReadoutsProperties>): ReactNode {
	const readouts = new Array<ReactNode>();
	for (const rival of RIVAL_NAMES) {
		readouts.push(<BenchmarkReadout key={rival} metric={metric} rival={rival} scenario={scenario} />);
	}

	return <dl className="readouts readouts--start">{readouts}</dl>;
}
