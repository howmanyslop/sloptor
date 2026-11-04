// Emission sites: parameters (plain, optional, defaulted, rest,
// destructured, `this`), LuaTuple and void returns, async functions and
// arrows, a generator, object literals with methods, and nested arrows with
// generics.
export function plain(a: number, b: string): boolean {
	return a > b.size();
}

export function optionalParams(a: number, b?: string, c = 2): number {
	return a + (b === undefined ? 0 : b.size()) + c;
}

export function restParams(first: number, ...rest: string[]): number {
	return first + rest.size();
}

export function restLoop(...values: number[]): number {
	let total = 0;
	for (const value of values) {
		total += value;
	}
	return total;
}

export function restArray(...rest: string[]): string[] {
	return rest;
}

export const named = function fact(n: number): number {
	return n <= 1 ? 1 : n * fact(n - 1);
};

export function destructured({ x, y }: { x: number; y: number }, [first]: [string]): string {
	return `${x + y}${first}`;
}

export function thisParam(this: { x: number }, n: number): number {
	return this.x + n;
}

export function luaTuple(): LuaTuple<[number, string]> {
	return $tuple(1, "a");
}

// Luau packs have no omittable elements: `(number, string?)` still wants two
// values, so the short return passes `undefined` explicitly.
export function optionalTuple(flag: boolean): LuaTuple<[number, string?]> {
	if (flag) {
		return $tuple(1, "a");
	}
	return $tuple(2, undefined);
}

export function nothing(): void {
	print("x");
}

export function bareReturn(flag: boolean): void {
	if (flag) {
		return;
	}
	print("x");
}

export async function fetched(n: number): Promise<number> {
	return n * 2;
}

export const fetchArrow = async (n: number) => n + 1;

export async function awaited(): Promise<number> {
	const value = await fetched(1);
	return value;
}

export async function pair(): Promise<[number, number]> {
	return [1, 2];
}

export async function summed(): Promise<number> {
	const [a, b] = await pair();
	return a + b;
}

export function* counter(limit: number): Generator<number, void, unknown> {
	for (let i = 0; i < limit; i++) {
		yield i;
	}
}

export interface Counter {
	count: number;
	inc(): void;
}

export const typed: Counter = {
	count: 0,
	inc() {
		this.count += 1;
	},
};

export const anonymous = {
	count: 0,
	inc() {
		this.count += 1;
	},
};

export const explicit = {
	count: 0,
	inc(this: Counter) {
		this.count += 1;
	},
};

export interface Options {
	host: string;
	port: number;
}

// `??` spills the literal to a temp; the temp keeps its shape and each
// `_condition` is re-bound to the non-nil type TS gives the expression.
export function merged(defaults: Options, partial: { host?: string; port?: number }): Options {
	return { host: partial.host ?? defaults.host, port: partial.port ?? defaults.port };
}

export function fallback(name: string | undefined): string {
	return name || "anonymous";
}

export const identity = <T,>(value: T): T => value;

export function nested(): (n: number) => number {
	const double = (n: number) => n * 2;
	return double;
}
