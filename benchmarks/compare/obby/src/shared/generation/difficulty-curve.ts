import { DIFFICULTY_ORDER } from "../types";
import type { DifficultyTier } from "../types";
import { clamp, lerp, smoothstep } from "../util/math";

export type CurveShape =
	| { readonly type: "linear" }
	| { readonly type: "ease"; readonly exponent: number }
	| { readonly type: "sawtooth"; readonly teeth: number; readonly relief: number }
	| { readonly type: "plateau"; readonly steps: ReadonlyArray<number> };

export interface CurveSample {
	readonly stageIndex: number;
	readonly progress: number;
	readonly difficulty: number;
	readonly tier: DifficultyTier;
}

const TIER_THRESHOLDS: ReadonlyArray<[upperBound: number, tier: DifficultyTier]> = [
	[0.12, "Tutorial"],
	[0.35, "Easy"],
	[0.6, "Medium"],
	[0.82, "Hard"],
	[math.huge, "Insane"],
];

export class DifficultyCurve {
	public static readonly DEFAULT_SHAPE: CurveShape = { type: "sawtooth", teeth: 4, relief: 0.12 };

	public constructor(
		private readonly shape: CurveShape = DifficultyCurve.DEFAULT_SHAPE,
		private readonly floor = 0.05,
		private readonly ceiling = 1,
	) {}

	public static tierFor(difficulty: number): DifficultyTier {
		const match = TIER_THRESHOLDS.find(([upperBound]) => difficulty < upperBound);
		return match?.[1] ?? "Insane";
	}

	public static tierRank(tier: DifficultyTier): number {
		return DIFFICULTY_ORDER.indexOf(tier);
	}

	public evaluate(progress: number): number {
		const t = clamp(progress, 0, 1);
		const shaped = this.applyShape(t);
		return lerp(this.floor, this.ceiling, clamp(shaped, 0, 1));
	}

	public sample(stageCount: number): Array<CurveSample> {
		const samples = new Array<CurveSample>();
		for (let stageIndex = 0; stageIndex < stageCount; stageIndex++) {
			const progress = stageCount <= 1 ? 0 : stageIndex / (stageCount - 1);
			const difficulty = this.evaluate(progress);
			samples.push({ stageIndex, progress, difficulty, tier: DifficultyCurve.tierFor(difficulty) });
		}
		return samples;
	}

	public withCeiling(ceiling: number): DifficultyCurve {
		return new DifficultyCurve(this.shape, this.floor, ceiling);
	}

	private applyShape(t: number): number {
		const shape = this.shape;
		switch (shape.type) {
			case "linear":
				return t;
			case "ease":
				return t ** shape.exponent;
			case "sawtooth": {
				const toothLength = 1 / shape.teeth;
				const inTooth = (t % toothLength) / toothLength;
				const relief = inTooth > 0.85 ? shape.relief * smoothstep(0.85, 1, inTooth) : 0;
				return t - relief;
			}
			case "plateau": {
				const steps = shape.steps;
				if (steps.isEmpty()) {
					return t;
				}
				const index = math.min(math.floor(t * steps.size()), steps.size() - 1);
				return steps[index];
			}
		}
	}
}

export function summarizeCurve(samples: ReadonlyArray<CurveSample>): Map<DifficultyTier, number> {
	const summary = new Map<DifficultyTier, number>();
	for (const { tier } of samples) {
		summary.set(tier, (summary.get(tier) ?? 0) + 1);
	}
	return summary;
}
