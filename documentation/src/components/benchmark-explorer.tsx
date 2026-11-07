import { useCallback, useMemo, useState } from "react";

import { getRows, isMetricName, METRICS } from "$data/benchmark-metrics";
import { isScenarioName } from "$data/benchmark-results";

import { BenchmarkFigure } from "./benchmark-figure";
import { BenchmarkReadouts } from "./benchmark-readouts";
import { usePrefersReducedMotion } from "./use-prefers-reduced-motion";

import type { ReactNode } from "react";

import type { MetricName } from "$data/benchmark-metrics";
import type { ScenarioName } from "$data/benchmark-results";

/**
 * The benchmarks page's interactive figure. It renders the figure column and
 * the readouts column side by side, so the page's row grid can place them, and
 * keeps the scenario and measurement they share.
 *
 * @returns The two columns.
 */
export function BenchmarkExplorer(): ReactNode {
	const [scenario, setScenario] = useState<ScenarioName>("cold");
	const [metricName, setMetricName] = useState<MetricName>("time");
	const animate = !usePrefersReducedMotion();

	const metric = METRICS[metricName];
	const rows = useMemo(() => getRows(scenario, metric), [scenario, metric]);

	const handleScenarioChange = useCallback((value: string): void => {
		if (isScenarioName(value)) {
			setScenario(value);
		}
	}, []);
	const handleMetricChange = useCallback((value: string): void => {
		if (isMetricName(value)) {
			setMetricName(value);
		}
	}, []);

	return (
		<>
			<BenchmarkFigure
				animate={animate}
				metricName={metricName}
				rows={rows}
				scenario={scenario}
				onMetricChange={handleMetricChange}
				onScenarioChange={handleScenarioChange}
			/>
			<BenchmarkReadouts metric={metric} scenario={scenario} />
		</>
	);
}
