export type DifficultyTier = "Tutorial" | "Easy" | "Medium" | "Hard" | "Insane";

export const DIFFICULTY_ORDER: ReadonlyArray<DifficultyTier> = ["Tutorial", "Easy", "Medium", "Hard", "Insane"];

export enum ObstacleKind {
	Platform = "Platform",
	MovingPlatform = "MovingPlatform",
	Spinner = "Spinner",
	Laser = "Laser",
	DisappearingTiles = "DisappearingTiles",
	KillBrick = "KillBrick",
	Conveyor = "Conveyor",
	Ladder = "Ladder",
	Checkpoint = "Checkpoint",
}

export const enum Direction {
	North,
	East,
	South,
	West,
	Up,
	Down,
}

export const enum CellState {
	Empty,
	Reserved,
	Solid,
	Hazard,
	Path,
}

export type GridPoint = [x: number, y: number, z: number];

export interface Bounds {
	readonly min: Vector3;
	readonly max: Vector3;
}

export interface ObstacleSpec {
	readonly id: string;
	readonly kind: ObstacleKind;
	readonly cell: GridPoint;
	readonly footprint: GridPoint;
	readonly rotation: number;
	readonly params: ObstacleParams;
}

export type ObstacleParams =
	| { readonly kind: ObstacleKind.Platform; readonly size: Vector3; readonly material: Enum.Material }
	| {
			readonly kind: ObstacleKind.MovingPlatform;
			readonly size: Vector3;
			readonly travel: Vector3;
			readonly period: number;
			readonly phase: number;
	  }
	| { readonly kind: ObstacleKind.Spinner; readonly armLength: number; readonly armCount: number; readonly rpm: number }
	| { readonly kind: ObstacleKind.Laser; readonly span: number; readonly onTime: number; readonly offTime: number }
	| {
			readonly kind: ObstacleKind.DisappearingTiles;
			readonly columns: number;
			readonly rows: number;
			readonly visibleTime: number;
			readonly hiddenTime: number;
	  }
	| { readonly kind: ObstacleKind.KillBrick; readonly size: Vector3; readonly pulse: boolean }
	| { readonly kind: ObstacleKind.Conveyor; readonly length: number; readonly speed: number }
	| { readonly kind: ObstacleKind.Ladder; readonly height: number; readonly rungSpacing: number }
	| { readonly kind: ObstacleKind.Checkpoint; readonly stageIndex: number };

export type ParamsOf<K extends ObstacleKind> = Extract<ObstacleParams, { readonly kind: K }>;

export interface SegmentSpec {
	readonly index: number;
	readonly tier: DifficultyTier;
	readonly entry: GridPoint;
	readonly exit: GridPoint;
	readonly obstacles: ReadonlyArray<ObstacleSpec>;
}

export interface StageSpec {
	readonly index: number;
	readonly tier: DifficultyTier;
	readonly difficulty: number;
	readonly segments: ReadonlyArray<SegmentSpec>;
	readonly checkpoint: ObstacleSpec;
	readonly worldOffset: Vector3;
	readonly estimatedSeconds: number;
}

export interface CourseSpec {
	readonly seed: number;
	readonly label: string;
	readonly stages: ReadonlyArray<StageSpec>;
	readonly generatedAt: number;
}

export function assertNever(value: never, context: string): never {
	throw `Unhandled variant in ${context}: ${tostring(value)}`;
}

export function directionToOffset(direction: Direction): GridPoint {
	switch (direction) {
		case Direction.North:
			return [0, 0, 1];
		case Direction.East:
			return [1, 0, 0];
		case Direction.South:
			return [0, 0, -1];
		case Direction.West:
			return [-1, 0, 0];
		case Direction.Up:
			return [0, 1, 0];
		case Direction.Down:
			return [0, -1, 0];
	}
}

export function gridPointKey([x, y, z]: GridPoint): string {
	return `${x},${y},${z}`;
}

export function addGridPoints(a: GridPoint, b: GridPoint): GridPoint {
	return [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
}

export function manhattan(a: GridPoint, b: GridPoint): number {
	return math.abs(a[0] - b[0]) + math.abs(a[1] - b[1]) + math.abs(a[2] - b[2]);
}
