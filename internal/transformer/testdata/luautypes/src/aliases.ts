// Aliases: exported and local interfaces, generic aliases with defaults,
// recursion with identical and with different arguments, a merged interface,
// aliases of a union, a literal and a callable, a nested namespace, a name
// that collides with a Roblox type, an `export { X }` specifier and a
// LuaTuple alias.
export interface Point {
	x: number;
	y: number;
}

interface Hidden {
	secret: number;
}

interface Listed {
	id: string;
}

export type { Listed };

export type Boxed<T = number> = { value: T };

export type Pair<A, B = A> = { first: A; second: B };

// `until` is a Luau keyword, so the parameter is left out of the alias and
// every reference to it (T's default included) widens to any.
export type Labeled<until = string, T = until> = { label: until; value: T };

export type Tree = { value: number; children: Tree[] };

export interface Chain<T> {
	value: T;
	next?: Chain<T>;
}

export type Weird<T> = { value: T; nested?: Weird<T[]> };

export interface Merged {
	a: number;
}

export interface Merged {
	b: string;
}

export type Mode = "r" | "w";

export type Literal = "x";

export type Handler = (value: number) => void;

export interface Part {
	id: number;
}

export namespace Geometry {
	export interface Size {
		width: number;
		height: number;
	}
}

export type Tuple = LuaTuple<[number, string]>;

export interface Holder {
	point: Point;
	hidden: Hidden;
	listed: Listed;
	boxed: Boxed;
	pair: Pair<number>;
	labeled: Labeled<string, number>;
	tree: Tree;
	chain: Chain<number>;
	weird: Weird<number>;
	merged: Merged;
	mode: Mode;
	literal: Literal;
	handler: Handler;
	part: Part;
	size: Geometry.Size;
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

export function size(): Geometry.Size {
	return { width: 1, height: 2 };
}

export function tuple(): Tuple {
	return $tuple(1, "a");
}
