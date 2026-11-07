import type { DifficultyTier } from "./types";

export interface PhysicsConfig {
	readonly gravity: number;
	readonly walkSpeed: number;
	readonly jumpPower: number;
	readonly hipHeight: number;
	readonly safetyMargin: number;
}

export interface CourseConfig {
	readonly stageCount: number;
	readonly segmentsPerStage: number;
	readonly gridCellSize: number;
	readonly gridWidth: number;
	readonly gridHeight: number;
	readonly gridDepth: number;
	readonly maxSolverIterations: number;
	readonly maxRegenerationAttempts: number;
}

export interface TierSettings {
	readonly displayName: string;
	readonly color: Color3;
	readonly obstacleBudget: number;
	readonly hazardRatio: number;
	readonly gapScale: number;
}

export const PHYSICS: PhysicsConfig = {
	gravity: 196.2,
	walkSpeed: 16,
	jumpPower: 50,
	hipHeight: 2,
	safetyMargin: 0.85,
};

export const COURSE: CourseConfig = {
	stageCount: 12,
	segmentsPerStage: 6,
	gridCellSize: 8,
	gridWidth: 12,
	gridHeight: 10,
	gridDepth: 48,
	maxSolverIterations: 4096,
	maxRegenerationAttempts: 8,
};

export const TIER_SETTINGS: Readonly<Record<DifficultyTier, TierSettings>> = {
	Tutorial: {
		displayName: "Warm Up",
		color: Color3.fromRGB(120, 200, 120),
		obstacleBudget: 3,
		hazardRatio: 0,
		gapScale: 0.55,
	},
	Easy: {
		displayName: "Easy",
		color: Color3.fromRGB(90, 170, 255),
		obstacleBudget: 5,
		hazardRatio: 0.15,
		gapScale: 0.65,
	},
	Medium: {
		displayName: "Medium",
		color: Color3.fromRGB(255, 200, 60),
		obstacleBudget: 7,
		hazardRatio: 0.3,
		gapScale: 0.75,
	},
	Hard: {
		displayName: "Hard",
		color: Color3.fromRGB(255, 120, 50),
		obstacleBudget: 9,
		hazardRatio: 0.45,
		gapScale: 0.85,
	},
	Insane: {
		displayName: "Insane",
		color: Color3.fromRGB(220, 40, 90),
		obstacleBudget: 12,
		hazardRatio: 0.6,
		gapScale: 0.93,
	},
};

export const DATASTORE_NAME = "ObbyProgress_v3";
export const LEADERBOARD_STORE_NAME = "ObbyDailyTimes_v1";
export const AUTOSAVE_INTERVAL = 90;
export const RESPAWN_DELAY = 1.5;
export const COURSE_FOLDER_NAME = "ProceduralCourse";
export const REMOTES_FOLDER_NAME = "ObbyRemotes";
