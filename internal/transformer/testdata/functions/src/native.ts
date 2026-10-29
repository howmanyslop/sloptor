//!native
export function addVector(a: vector, b: vector): vector {
	return a;
}

export function maybeAddVector(a: vector, b?: vector): vector {
	return b === undefined ? a : b;
}

export function addVector3(a: Vector3, b: Vector3): Vector3 {
	return a.add(b);
}

export function scale(x: number, factor = 2, label?: string): number {
	return x * factor;
}

export function nameOf(value: string | undefined): string | undefined {
	return value;
}

export function mixed(value: number | string, flag: boolean, anything: unknown) {
	return [value, flag, anything];
}

export function sum(values: number[], lookup: Map<string, number>, options: { size: number }): number {
	return values.size() + lookup.size() + options.size;
}

export function sumGeneric<T extends number>(value: T): T {
	return value;
}

export async function loadAsync(count: number): Promise<number> {
	return count;
}

export function callback(fn: (n: number) => number) {
	return fn(1);
}

export const arrow = (n: number): number => n + 1;

export class Counter {
	public constructor(public readonly start: number) {}

	public next(step: number, other?: Counter): number {
		return this.start + step;
	}

	public static make(start: number): Counter {
		return new Counter(start);
	}
}
