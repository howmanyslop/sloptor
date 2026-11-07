export const ARENA_SIZE = new Vector3(200, 1, 200);
export const ARENA_ORIGIN = new CFrame(0, 10, 0);
export const SPAWN_HEIGHT = 5;
export const ROUND_LENGTH = 120;
export const INTERMISSION_LENGTH = 15;
export const COIN_TAG = "Coin";
export const HAZARD_TAG = "Hazard";
export const PLATFORM_TAG = "MovingPlatform";
export const CHECKPOINT_TAG = "Checkpoint";

export const TEAM_COLORS: ReadonlyArray<Color3> = [
	Color3.fromRGB(231, 76, 60),
	Color3.fromRGB(52, 152, 219),
	Color3.fromRGB(46, 204, 113),
	Color3.fromRGB(241, 196, 15),
];

export const PALETTE = {
	background: Color3.fromRGB(24, 26, 32),
	panel: Color3.fromRGB(36, 39, 48),
	accent: Color3.fromRGB(255, 170, 60),
	text: Color3.fromRGB(240, 240, 245),
	muted: Color3.fromRGB(140, 145, 160),
	danger: Color3.fromRGB(220, 60, 60),
} as const;

export enum GamePhase {
	Waiting = "Waiting",
	Intermission = "Intermission",
	Playing = "Playing",
	Results = "Results",
}

export enum StatName {
	Coins = "Coins",
	Wins = "Wins",
	Deaths = "Deaths",
}

export const ATTRIBUTES = {
	value: "Value",
	speed: "Speed",
	damage: "Damage",
	team: "Team",
	phase: "Phase",
	checkpoint: "Checkpoint",
} as const;
