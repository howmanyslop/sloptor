import { ObstacleKind, assertNever } from "../types";
import { lerp } from "../util/math";
import { WeightedPicker } from "../util/weighted-picker";

import type { DifficultyTier, ObstacleParams } from "../types";
import type { SeededRandom } from "../util/random";

export interface BlueprintContext {
	readonly random: SeededRandom;
	readonly difficulty: number;
	readonly tier: DifficultyTier;
	readonly stageIndex: number;
}

export type PlaceableKind = Exclude<ObstacleKind, ObstacleKind.Checkpoint>;

interface KindProfile {
	readonly kind: PlaceableKind;
	readonly baseWeight: number;
	readonly minDifficulty: number;
	readonly hazard: boolean;
	readonly footprint: [x: number, y: number, z: number];
}

export const KIND_PROFILES: ReadonlyArray<KindProfile> = [
	{ kind: ObstacleKind.Platform, baseWeight: 10, minDifficulty: 0, hazard: false, footprint: [1, 1, 1] },
	{ kind: ObstacleKind.MovingPlatform, baseWeight: 5, minDifficulty: 0.15, hazard: false, footprint: [1, 1, 2] },
	{ kind: ObstacleKind.Spinner, baseWeight: 4, minDifficulty: 0.3, hazard: true, footprint: [2, 1, 2] },
	{ kind: ObstacleKind.Laser, baseWeight: 3, minDifficulty: 0.4, hazard: true, footprint: [1, 1, 1] },
	{ kind: ObstacleKind.DisappearingTiles, baseWeight: 4, minDifficulty: 0.25, hazard: false, footprint: [1, 1, 2] },
	{ kind: ObstacleKind.KillBrick, baseWeight: 5, minDifficulty: 0.1, hazard: true, footprint: [1, 1, 1] },
	{ kind: ObstacleKind.Conveyor, baseWeight: 3, minDifficulty: 0.2, hazard: false, footprint: [1, 1, 2] },
	{ kind: ObstacleKind.Ladder, baseWeight: 2, minDifficulty: 0.05, hazard: false, footprint: [1, 2, 1] },
];

const PROFILE_BY_KIND = new Map<PlaceableKind, KindProfile>(KIND_PROFILES.map((profile) => [profile.kind, profile]));

export function profileOf(kind: PlaceableKind): KindProfile {
	return PROFILE_BY_KIND.get(kind)!;
}

export function buildKindPicker(difficulty: number, hazardRatio: number): WeightedPicker<PlaceableKind> {
	const picker = new WeightedPicker<PlaceableKind>();
	for (const { kind, baseWeight, minDifficulty, hazard } of KIND_PROFILES) {
		if (difficulty < minDifficulty) {
			continue;
		}
		const ramp = 1 + (difficulty - minDifficulty) * 2;
		const hazardFactor = hazard ? hazardRatio * 2 : 1 - hazardRatio * 0.5;
		picker.add(kind, baseWeight * ramp * hazardFactor);
	}
	return picker;
}

const PLATFORM_MATERIALS: ReadonlyArray<Enum.Material> = [
	Enum.Material.SmoothPlastic,
	Enum.Material.Wood,
	Enum.Material.Slate,
	Enum.Material.Ice,
	Enum.Material.Grass,
];

export function generateParams(kind: PlaceableKind, context: BlueprintContext): ObstacleParams {
	const { random, difficulty } = context;
	const scale = (easy: number, hard: number) => lerp(easy, hard, difficulty);

	switch (kind) {
		case ObstacleKind.Platform: {
			const side = math.max(3, scale(8, 4) + random.nextRange(-1, 1));
			const materials =
				difficulty > 0.7
					? PLATFORM_MATERIALS
					: PLATFORM_MATERIALS.filter((material) => material !== Enum.Material.Ice);
			return {
				kind,
				size: new Vector3(side, 1, side),
				material: random.pick(materials) ?? Enum.Material.SmoothPlastic,
			};
		}
		case ObstacleKind.MovingPlatform: {
			const axis = random.chance(0.5) ? new Vector3(1, 0, 0) : new Vector3(0, 1, 0);
			return {
				kind,
				size: new Vector3(scale(7, 4), 1, scale(7, 4)),
				travel: axis.mul(scale(6, 14) * random.nextSign()),
				period: scale(5, 2.2),
				phase: random.nextFloat(),
			};
		}
		case ObstacleKind.Spinner:
			return {
				kind,
				armLength: scale(5, 9),
				armCount: random.nextInt(difficulty > 0.6 ? 2 : 1, difficulty > 0.8 ? 4 : 2),
				rpm: scale(12, 36) * random.nextSign(),
			};
		case ObstacleKind.Laser: {
			const cycle = scale(4, 2);
			const onFraction = scale(0.3, 0.6);
			return {
				kind,
				span: random.nextRange(8, 14),
				onTime: cycle * onFraction,
				offTime: cycle * (1 - onFraction),
			};
		}
		case ObstacleKind.DisappearingTiles:
			return {
				kind,
				columns: random.nextInt(1, 3),
				rows: random.nextInt(2, math.floor(scale(3, 6))),
				visibleTime: scale(3, 1.2),
				hiddenTime: scale(1, 1.8),
			};
		case ObstacleKind.KillBrick: {
			const width = random.nextRange(4, scale(6, 12));
			return {
				kind,
				size: new Vector3(width, 1, random.nextRange(2, 6)),
				pulse: difficulty > 0.5 && random.chance(0.4),
			};
		}
		case ObstacleKind.Conveyor: {
			const reverse = difficulty > 0.45 && random.chance(difficulty * 0.6);
			return { kind, length: random.nextRange(12, 24), speed: scale(8, 22) * (reverse ? -1 : 1) };
		}
		case ObstacleKind.Ladder:
			return { kind, height: random.nextRange(8, scale(12, 24)), rungSpacing: scale(1.5, 2.5) };
		default:
			return assertNever(kind, "generateParams");
	}
}
