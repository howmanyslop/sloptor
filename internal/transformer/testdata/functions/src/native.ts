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

export function arrayTypes(values: number[], readonlyValues: ReadonlyArray<string>, pair: [string, number]): number[] {
	return [values.size() + readonlyValues.size() + pair.size()];
}

export function collectionTypes(lookup: Map<string, number>, keys: Set<string>, counts: Record<string, number>): number {
	return lookup.size() + keys.size() + counts["a"];
}

export interface Foo {
	a: number;
	b?: string;
}

export type Balls = { x: number; y: Vector3 };

export function structs(foo: Foo, balls: Balls, maybe?: Foo): Foo {
	return foo;
}

export function nested(index: Map<string, number[]>): Map<string, number[]> {
	return index;
}

export interface LinkedNode {
	value: number;
	next?: LinkedNode;
}

export interface WithMethod {
	value: number;
	run(): number;
}

export function linked(node: LinkedNode, thing: WithMethod): number {
	return node.value + thing.run();
}

export interface Quirky {
	"kebab-case": number;
	1: string;
}

export function quirky(value: Quirky): Quirky {
	return value;
}

export function recordTypes(numbers: Record<number, string>): Record<number, string> {
	return numbers;
}

export type Both = { [key: string]: number; [index: number]: number };

export function bothIndex(index: Both): Both {
	return index;
}

export function unionTables(value: number[] | string[]): number[] | string[] {
	return value;
}

export function instanceOf(value: Instance): Instance {
	return value;
}

export function unionOptional(value?: number[] | string[]): number {
	return value === undefined ? 0 : value.size();
}
