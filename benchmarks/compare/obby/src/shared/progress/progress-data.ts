export const CURRENT_VERSION = 3;

export interface StageRecord {
	readonly bestSeconds: number;
	readonly deaths: number;
	readonly completions: number;
}

export interface DailyRecord {
	readonly key: string;
	readonly bestSeconds: number;
	readonly attempts: number;
}

export interface ProgressV1 {
	readonly version: 1;
	readonly stage: number;
	readonly coins: number;
}

export interface ProgressV2 {
	readonly version: 2;
	readonly highestStage: number;
	readonly coins: number;
	readonly totalDeaths: number;
}

export interface ProgressV3 {
	readonly version: 3;
	readonly highestStage: number;
	readonly coins: number;
	readonly totalDeaths: number;
	readonly totalJumps: number;
	readonly stageRecords: Readonly<Record<string, StageRecord>>;
	readonly dailies: ReadonlyArray<DailyRecord>;
	readonly settings: PlayerSettings;
}

export interface PlayerSettings {
	readonly showTimer: boolean;
	readonly showGhostPath: boolean;
	readonly reducedMotion: boolean;
}

export type AnyProgress = ProgressV1 | ProgressV2 | ProgressV3;
export type PlayerProgress = ProgressV3;

export const DEFAULT_SETTINGS: PlayerSettings = {
	showTimer: true,
	showGhostPath: false,
	reducedMotion: false,
};

export function defaultProgress(): PlayerProgress {
	return {
		version: 3,
		highestStage: 0,
		coins: 0,
		totalDeaths: 0,
		totalJumps: 0,
		stageRecords: {},
		dailies: [],
		settings: DEFAULT_SETTINGS,
	};
}

function migrateOnce(data: AnyProgress): AnyProgress {
	switch (data.version) {
		case 1:
			return { version: 2, highestStage: data.stage, coins: data.coins, totalDeaths: 0 };
		case 2:
			return {
				...defaultProgress(),
				highestStage: data.highestStage,
				coins: data.coins,
				totalDeaths: data.totalDeaths,
			};
		case 3:
			return data;
	}
}

export function migrate(data: AnyProgress): PlayerProgress {
	let current = data;
	while (current.version !== CURRENT_VERSION) {
		current = migrateOnce(current);
	}
	return current as PlayerProgress;
}

function isRecord(value: unknown): value is Record<string, unknown> {
	return typeIs(value, "table");
}

/** Validate untrusted DataStore payloads before trusting their shape. */
export function parseProgress(raw: unknown): PlayerProgress | undefined {
	if (!isRecord(raw)) {
		return undefined;
	}
	const version = raw.version;
	if (!typeIs(version, "number") || version < 1 || version > CURRENT_VERSION) {
		return undefined;
	}
	return migrate(raw as unknown as AnyProgress);
}

export function recordStageTime(progress: PlayerProgress, stageIndex: number, seconds: number, deaths: number): PlayerProgress {
	const key = tostring(stageIndex);
	const previous = progress.stageRecords[key];
	const record: StageRecord = {
		bestSeconds: previous ? math.min(previous.bestSeconds, seconds) : seconds,
		deaths: (previous?.deaths ?? 0) + deaths,
		completions: (previous?.completions ?? 0) + 1,
	};
	return {
		...progress,
		highestStage: math.max(progress.highestStage, stageIndex + 1),
		stageRecords: { ...progress.stageRecords, [key]: record },
	};
}

export function recordDaily(progress: PlayerProgress, key: string, seconds: number): PlayerProgress {
	const existing = progress.dailies.find((daily) => daily.key === key);
	const others = progress.dailies.filter((daily) => daily.key !== key);
	const updated: DailyRecord = existing
		? { key, bestSeconds: math.min(existing.bestSeconds, seconds), attempts: existing.attempts + 1 }
		: { key, bestSeconds: seconds, attempts: 1 };
	// Keep a rolling two-week window so the payload stays small.
	const dailies = [updated, ...others].sort((a, b) => a.key > b.key);
	const trimmed = new Array<DailyRecord>();
	for (let index = 0; index < math.min(14, dailies.size()); index++) {
		trimmed.push(dailies[index]);
	}
	return { ...progress, dailies: trimmed };
}

export function bestDailyTime(progress: PlayerProgress, key: string): number | undefined {
	return progress.dailies.find((daily) => daily.key === key)?.bestSeconds;
}
