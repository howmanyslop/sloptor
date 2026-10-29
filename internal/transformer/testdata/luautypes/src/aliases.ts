// Placeholder for WP5: an exported interface, a generic alias with a default,
// a recursive alias and a merged interface.
export interface Point {
	x: number;
	y: number;
}

export type Boxed<T = number> = { value: T };

export type Tree = { value: number; children: Tree[] };

export interface Merged {
	a: number;
}

export interface Merged {
	b: string;
}

export function origin(): Point {
	return { x: 0, y: 0 };
}

export function box<T>(value: T): Boxed<T> {
	return { value };
}

export function leaf(value: number): Tree {
	return { value, children: [] };
}

export function merged(): Merged {
	return { a: 1, b: "b" };
}
