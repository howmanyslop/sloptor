import type { TierSettings } from "../config";
import type { CellState, DifficultyTier, GridPoint, ObstacleSpec, SegmentSpec } from "../types";
import type { Grid3 } from "../util/grid";
import type { SeededRandom } from "../util/random";
import type { PathNode } from "./layout-solver";

export const enum DiagnosticLevel {
	Info,
	Warning,
	Failure,
}

export interface Diagnostic {
	readonly level: DiagnosticLevel;
	readonly step: string;
	readonly message: string;
}

export interface StageContext {
	readonly stageIndex: number;
	readonly tier: DifficultyTier;
	readonly difficulty: number;
	readonly settings: TierSettings;
	readonly random: SeededRandom;
	readonly grid: Grid3<CellState>;
	readonly worldOffset: Vector3;
	entry: GridPoint;
	exit: GridPoint;
	path: Array<PathNode>;
	obstacles: Array<ObstacleSpec>;
	segments: Array<SegmentSpec>;
	checkpoint?: ObstacleSpec;
	estimatedSeconds: number;
	readonly diagnostics: Array<Diagnostic>;
}

export function makeObstacleId(stageIndex: number, ordinal: number, suffix = ""): string {
	return `s${stageIndex}_o${ordinal}${suffix !== "" ? `_${suffix}` : ""}`;
}

export function report(context: StageContext, level: DiagnosticLevel, step: string, message: string): void {
	context.diagnostics.push({ level, step, message });
}

export function hasFailures(context: StageContext): boolean {
	return context.diagnostics.some((diagnostic) => diagnostic.level === DiagnosticLevel.Failure);
}
