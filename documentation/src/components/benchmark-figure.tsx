import { METRIC_NAMES, METRICS } from "$data/benchmark-metrics";
import { SCENARIO_LABELS, SCENARIO_NAMES } from "$data/benchmark-results";

import { BenchmarkChart } from "./benchmark-chart";
import { SegmentedControl } from "./segmented-control";

import type { ReactNode } from "react";

import type { ChartRow, MetricName } from "$data/benchmark-metrics";
import type { ScenarioName } from "$data/benchmark-results";

interface BenchmarkFigureProperties {
	readonly animate: boolean;
	readonly metricName: MetricName;
	readonly onMetricChange: (value: string) => void;
	readonly onScenarioChange: (value: string) => void;
	readonly rows: Array<ChartRow>;
	readonly scenario: ScenarioName;
}

const SCENARIO_OPTIONS = SCENARIO_NAMES.map((name) => ({ label: SCENARIO_LABELS[name], value: name }));
const METRIC_OPTIONS = METRIC_NAMES.map((name) => ({ label: METRICS[name].label, value: name }));

/**
 * The figure column: two controls, an inline legend, and the chart in its
 * well.
 *
 * @param properties - The selection, the rows to draw, and the change handlers.
 * @returns The column.
 */
export function BenchmarkFigure({
	animate,
	metricName,
	onMetricChange,
	onScenarioChange,
	rows,
	scenario,
}: Readonly<BenchmarkFigureProperties>): ReactNode {
	const description = `Bar chart of how many times as much ${METRICS[metricName].noun} rbxtsgo and rbxtsc need as sloptor on each of ${rows.length} projects. Every bar starts at zero. The table below lists every value.`;

	return (
		<div className="note-body">
			<div className="bench-controls">
				<SegmentedControl
					label="Scenario"
					options={SCENARIO_OPTIONS}
					value={scenario}
					onChange={onScenarioChange}
				/>
				<SegmentedControl
					label="Measurement"
					options={METRIC_OPTIONS}
					value={metricName}
					onChange={onMetricChange}
				/>
			</div>
			<ul className="figure-legend">
				<li className="legend-item legend-item--sloptor">{"sloptor, 1×"}</li>
				<li className="legend-item legend-item--rbxtsgo">{"rbxtsgo"}</li>
				<li className="legend-item legend-item--rbxtsc">{"rbxtsc"}</li>
			</ul>
			<figure className="bench-well">
				<div aria-hidden="true">
					<BenchmarkChart animate={animate} rows={rows} />
				</div>
				<figcaption className="sr-only">{description}</figcaption>
			</figure>
		</div>
	);
}
