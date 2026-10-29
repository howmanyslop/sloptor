import { aNumber } from "./primitives";

// Tables: interfaces, aliases, index signatures, arrays, tuples, maps, sets,
// classes, enums and `typeof`.
export interface Point {
	x: number;
	y: number;
}

export interface Shape {
	readonly kind: string;
	area(): number;
	optional?: string;
	"kebab-case": number;
	1: boolean;
	end: string;
	onEvent: (value: number) => void;
	maybeRun?(): void;
	explicitThis(this: Point, n: number): number;
	clone(): this;
}

export interface Dict {
	[key: string]: number;
}

export interface NumDict {
	[index: number]: string;
}

export interface Both {
	[key: string]: number;
	[index: number]: number;
}

export interface BothDiffer {
	[key: string]: number | string;
	[index: number]: string;
}

export interface ReadonlyDict {
	readonly [key: string]: number;
}

export declare const record: Record<string, number>;
export declare const literalRecord: Record<"a" | "b", number>;
export declare const partial: Partial<Point>;
export declare const readonlyPoint: Readonly<Point>;

export declare const numbers: number[];
export declare const readonlyNumbers: ReadonlyArray<string>;
export declare const readonlyArraySyntax: readonly boolean[];
export declare const pair: [string, number];
export declare const readonlyPair: readonly [string, number];
export declare const samePair: [number, number];

export declare const map: Map<string, Point>;
export declare const readonlyMap: ReadonlyMap<string, number>;
export declare const weakMap: WeakMap<Point, number>;
export declare const set: Set<string>;
export declare const readonlySet: ReadonlySet<number>;
export declare const weakSet: WeakSet<Point>;

export declare const nested: { inner: { deep: { value: number } } };
export declare const inlineMethod: { run(): number };

export interface Node {
	value: number;
	next?: Node;
}

export interface Tree<T> {
	value: T;
	children: Array<Tree<T>>;
}

export type Pair<T> = { first: T; second: T };
export declare const numberPair: Pair<number>;

export type Mode = "r" | "w";
export declare const mode: Mode;

export type Callback<T> = (value: T) => void;
export declare const callback: Callback<string>;

export interface Empty {}
export declare const empty: Empty;

export enum Direction {
	Up = "up",
	Down = "down",
}

export enum Codes {
	A = 1,
	B = 2,
}

export declare const direction: Direction;
export declare const code: Codes;
export declare const up: Direction.Up;
export declare const directionObject: typeof Direction;

export class Foo {
	public static self: typeof Foo;

	public constructor(public readonly x: number) {}

	public get(): number {
		return this.x;
	}

	public static make(): Foo {
		return new Foo(1);
	}
}

export declare const foo: Foo;
export declare const fooClass: typeof Foo;

export class Box<T> {
	public constructor(public value: T) {}
}

export declare const box: Box<number>;

export const config = { depth: 1 };
export declare const configType: typeof config;
export declare const importedType: typeof aNumber;
export let counter = 0;
export declare const counterType: typeof counter;
export declare const laterType: typeof later;
export const later = { x: 1 };
