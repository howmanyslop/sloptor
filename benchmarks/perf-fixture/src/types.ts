import type { Identified } from "./entities";

export type Result<T, E> = { ok: true; value: T } | { ok: false; error: E };

export interface Repository<T extends Identified> {
	find(id: number): T | undefined;
	all(): T[];
}

export type Keys<T> = { [K in keyof T]: K };
export type Nullable<T> = T | undefined;

export function isOk<T, E>(result: Result<T, E>): result is { ok: true; value: T } {
	return result.ok;
}
