import NumberFlow from "@number-flow/react";

import { getAverage } from "$data/benchmark-metrics";
import { formatRatio, getReport } from "$data/benchmark-results";

import type { ReactNode } from "react";

import type { Metric } from "$data/benchmark-metrics";
import type { RivalName, ScenarioName } from "$data/benchmark-results";

interface BenchmarkReadoutProperties {
	metric: Metric;
	rival: RivalName;
	scenario: ScenarioName;
}

const NUMBER_FORMAT = { maximumFractionDigits: 2, minimumFractionDigits: 2 };
const DIGIT_TIMING = { duration: 300, easing: "cubic-bezier(0.23, 1, 0.32, 1)" };
const FADE_TIMING = { duration: 200, easing: "ease-out" };

/**
 * One rival's headline ratio. The digits roll when the scenario or the
 * measurement changes, instead of swapping.
 *
 * @param properties - What to measure, for which rival, in which scenario.
 * @returns A label, the animated ratio, and a note on its range.
 */
export function BenchmarkReadout({ metric, rival, scenario }: Readonly<BenchmarkReadoutProperties>): ReactNode {
	const { high, low, mean } = getAverage(scenario, metric, rival);
	const projects = getReport(scenario).comparisons.length;
	const note = `How many times as much ${metric.noun} ${rival} needs as sloptor, averaged over ${projects} projects. It ranges from ${formatRatio(low)}× to ${formatRatio(high)}×.`;

	return (
		<div className="readout readout--lead">
			<dt>{`${rival} · ${metric.noun}`}</dt>
			<dd>
				<NumberFlow
					format={NUMBER_FORMAT}
					opacityTiming={FADE_TIMING}
					spinTiming={DIGIT_TIMING}
					suffix="×"
					transformTiming={DIGIT_TIMING}
					value={mean}
				/>
			</dd>
			<p className="readout-note">{note}</p>
		</div>
	);
}
