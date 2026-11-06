import type { Result } from "./types";

export function requireRange(value: number, min: number, max: number): Result<number, string> {
	if (value < min || value > max) {
		return { error: `${value} outside [${min}, ${max}]`, ok: false };
	}
	return { ok: true, value };
}

export function requireNonEmpty(value: string): Result<string, string> {
	if (value.length === 0) {
		return { error: "empty", ok: false };
	}
	return { ok: true, value };
}

export function isAllOk<TOk, TError>(results: Array<Result<TOk, TError>>): boolean {
	for (const result of results) {
		if (!result.ok) return false;
	}
	return true;
}
