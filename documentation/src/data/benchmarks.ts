export interface Benchmark {
	readonly files: number;
	readonly label: string;
	readonly milliseconds: number;
}

/** Timings from the README, measured on real production rbxts games. */
export const BENCHMARKS: ReadonlyArray<Benchmark> = [
	{ files: 222, label: "Typecheck", milliseconds: 161 },
	{ files: 95, label: "Full build", milliseconds: 355 },
	{ files: 95, label: "Watch rebuild", milliseconds: 180 },
];

export const FULL_BUILD_MILLISECONDS = 355;
