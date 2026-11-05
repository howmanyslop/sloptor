import type { Identified } from "./entities";

export type Result<TOk, TError> = { error: TError; ok: false } | { ok: true; value: TOk };

export interface Repository<TIdentified extends Identified> {
	all(): Array<TIdentified>;
	find(id: number): TIdentified | undefined;
}

export type Keys<TObject> = { [TKey in keyof TObject]: TKey };
export type Nullable<TValue> = TValue | undefined;

export function isOk<TOk, TError>(result: Result<TOk, TError>): result is { ok: true; value: TOk } {
	return result.ok;
}
