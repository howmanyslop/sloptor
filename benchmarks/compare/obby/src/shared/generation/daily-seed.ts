import { combineHashes, fnv1a } from "../util/bits";
import { dateKey } from "../util/format";
import type { CurveShape } from "./difficulty-curve";

const SECONDS_PER_DAY = 86400;
const SALT = "obby-daily-v2";

export const enum Weekday {
	Sunday = 1,
	Monday,
	Tuesday,
	Wednesday,
	Thursday,
	Friday,
	Saturday,
}

export interface DailyCourseInfo {
	readonly key: string;
	readonly seed: number;
	readonly dayNumber: number;
	readonly weekday: Weekday;
	readonly theme: DailyTheme;
	readonly curve: CurveShape;
	readonly stageCount: number;
	readonly secondsUntilReset: number;
}

export interface DailyTheme {
	readonly name: string;
	readonly accent: Color3;
	readonly skyTint: Color3;
}

const THEMES: ReadonlyArray<DailyTheme> = [
	{ name: "Neon Night", accent: Color3.fromRGB(0, 255, 200), skyTint: Color3.fromRGB(20, 10, 40) },
	{ name: "Sunset Ridge", accent: Color3.fromRGB(255, 140, 60), skyTint: Color3.fromRGB(255, 190, 150) },
	{ name: "Glacier", accent: Color3.fromRGB(150, 220, 255), skyTint: Color3.fromRGB(200, 230, 255) },
	{ name: "Volcano", accent: Color3.fromRGB(255, 60, 30), skyTint: Color3.fromRGB(80, 30, 20) },
	{ name: "Candy", accent: Color3.fromRGB(255, 120, 220), skyTint: Color3.fromRGB(255, 220, 240) },
	{ name: "Forest", accent: Color3.fromRGB(80, 200, 90), skyTint: Color3.fromRGB(180, 230, 190) },
	{ name: "Monochrome", accent: Color3.fromRGB(230, 230, 230), skyTint: Color3.fromRGB(60, 60, 60) },
];

/** Weekly rhythm: gentle start of the week, spike on the weekend. */
function curveFor(weekday: Weekday): [shape: CurveShape, stageCount: number] {
	switch (weekday) {
		case Weekday.Monday:
		case Weekday.Tuesday:
			return [{ type: "ease", exponent: 1.6 }, 10];
		case Weekday.Wednesday:
			return [{ type: "linear" }, 12];
		case Weekday.Thursday:
		case Weekday.Friday:
			return [{ type: "sawtooth", teeth: 4, relief: 0.15 }, 12];
		case Weekday.Saturday:
			return [{ type: "sawtooth", teeth: 3, relief: 0.08 }, 16];
		case Weekday.Sunday:
			return [{ type: "plateau", steps: [0.1, 0.3, 0.5, 0.7, 0.9] }, 15];
	}
}

export function dailyCourseFor(timestamp: number): DailyCourseInfo {
	const dayNumber = math.floor(timestamp / SECONDS_PER_DAY);
	const key = dateKey(timestamp);
	const seed = combineHashes(fnv1a(SALT), fnv1a(key), dayNumber);
	const weekday = os.date("!*t", timestamp).wday as Weekday;
	const [curve, stageCount] = curveFor(weekday);
	const theme = THEMES[seed % THEMES.size()];
	return {
		key,
		seed,
		dayNumber,
		weekday,
		theme,
		curve,
		stageCount,
		secondsUntilReset: (dayNumber + 1) * SECONDS_PER_DAY - timestamp,
	};
}

export function* upcomingDailies(from: number, days: number): Generator<DailyCourseInfo> {
	for (let offset = 0; offset < days; offset++) {
		yield dailyCourseFor(from + offset * SECONDS_PER_DAY);
	}
}
