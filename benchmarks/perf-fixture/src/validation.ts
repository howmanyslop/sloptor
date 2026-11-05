import type { Result } from "./types";

export function requireRange(value: number, min: number, max: number): Result<number, string> {
	if (value < min || value > max) {
		return { ok: false, error: `${value} outside [${min}, ${max}]` };
	}
	return { ok: true, value };
}

export function requireNonEmpty(value: string): Result<string, string> {
	if (value.length === 0) {
		return { ok: false, error: "empty" };
	}
	return { ok: true, value };
}

export function allOk<T, E>(results: Array<Result<T, E>>): boolean {
	for (let i = 0; i < results.length; i++) {
		if (!results[i].ok) return false;
	}
	return true;
}
