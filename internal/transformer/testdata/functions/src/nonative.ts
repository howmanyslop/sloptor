export function addVector(a: vector, b: vector): vector {
	return a;
}

export function maybeAddVector(a: vector, b?: vector): vector {
	return b === undefined ? a : b;
}

export function addVector3(a: Vector3, b: Vector3): Vector3 {
	return a.add(b);
}

export function lengthOf(value: vector | Vector3): number {
	return 1;
}

export function scale(x: number, factor = 2, label?: string): number {
	return x * factor;
}

export function nameOf(value: string | undefined): string | undefined {
	return value;
}

// Where the hit landed, merged across two declarations.
export interface Hi { readonly hi: string; }
export interface Hi { readonly there: "there" }
export function getHi(): Hi { return { hi: "hi", there: "there" }; }
void getHi()

export function usesLater(): Later {
	return { value: 1 };
}

export interface Later {
	value: number;
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

export class Box<T> {
	public constructor(public readonly value: T) {}

	public get(): T {
		return this.value;
	}

	public boxed(): { inner: T } {
		return { inner: this.value };
	}
}

export function boxed(box: Box<number>): number {
	return box.get();
}

export function ownGeneric<K>(key: K): K {
	return key;
}

export function outerGeneric<K>(key: K): (other: K) => void {
	return (other: K): void => {};
}

export function functionReturn(make: (n: number) => (s: string) => boolean): (n: number) => (s: string) => boolean {
	return make;
}

export interface Handler {
	onEvent: (value: number) => void;
	readonly optional?: (value: string) => number;
}

export function handlers(handler?: Handler): void {
	if (handler !== undefined && handler.onEvent !== undefined) {
		handler.onEvent(1);
	}
}

export function robloxTypes(lookup: Map<string, Vector3>, part: Part, nested: { part: Part; flag: boolean }): Map<string, Vector3> {
	return lookup;
}

export interface Node {
	value: number;
}

namespace Inner {
	export interface Node {
		text: string;
	}
}

export function collision(a: Node, b: Inner.Node): number {
	return a.value + b.text.size();
}

export type Notify = (value: number) => void;

export type Callback<T> = (value: T) => void;

export function notify(cb: Notify, generic: Callback<string>): void {
	cb(1);
	generic("x");
}

export interface table {
	value: number;
}

export function reserved(value: table): number {
	return value.value;
}

declare function getPair(): LuaTuple<[number, string]>;
declare function getOptionalPair(): LuaTuple<[number, string?]>;
declare function getRestPair(): LuaTuple<[number, ...string[]]>;
declare function getEmptyPair(): LuaTuple<[]>;

export function multiReturn(): LuaTuple<[number, string]> {
	return getPair();
}

export function optionalReturn(): LuaTuple<[number, string?]> {
	return getOptionalPair();
}

export function restReturn(): LuaTuple<[number, ...string[]]> {
	return getRestPair();
}

export function emptyReturn(): LuaTuple<[]> {
	return getEmptyPair();
}

export function maybeReturn(): LuaTuple<[number, string]> | undefined {
	return getPair();
}

export function looseTupleCallback(cb: () => LuaTuple<string[]>): number {
	return cb().size();
}

export function tupleCallback(cb: () => LuaTuple<[number, string]>): number {
	return cb()[0];
}

export function maybeTupleCallback(cb?: (() => LuaTuple<[number, string]>) | undefined): void {
	cb?.();
}

export interface TupleHolder {
	pair: () => LuaTuple<[number, string]>;
	empty: () => LuaTuple<[]>;
}

export function tupleHolders(holder: TupleHolder): number {
	holder.empty();
	return holder.pair()[0];
}

export interface FooFactory {
	new (x: number): Foo;
}

export function factory(f: FooFactory): Foo {
	return new f(1);
}

export interface CtorHolder {
	create: new (x: number) => Foo;
}

export function ctorHolders(holder: CtorHolder): Foo {
	return new holder.create(1);
}

export function makeCounter(): typeof Counter {
	return Counter;
}

export interface Overloaded {
	(value: number): string;
	(value: string): number;
}

export function overloadedParam(fn: Overloaded): number {
	return fn(1).size();
}

export interface OverloadedMethod {
	run(value: number): string;
	run(value: string): number;
}

export function overloadedMethod(thing: OverloadedMethod): number {
	return thing.run(1).size();
}

export function genericCallback(identify: <T>(value: T) => T): number {
	return identify(1);
}

export interface IdentityHolder {
	identify: <T>(value: T) => T;
	replace: <T, U>(value: T, next: U) => U;
}

export function identityHolders(holder: IdentityHolder): number {
	holder.identify(1);
	return holder.replace(1, "a").size();
}

export interface Literals {
	a: "x" | "y";
	b?: "x" | "y";
	c: true;
	d: false;
	e: boolean;
	f: true | false | undefined;
	g: number;
	lit: "solo";
	count: 1 | 2;
	escaped: "a\"b\\c";
	format: (value: "x" | "y") => "x" | "y";
	template: `hi${string}`;
}

export enum Direction {
	Up = "up",
	Down = "down",
}

export enum Codes {
	A = 1,
	B = 2,
}

export interface EnumHolder {
	direction: Direction;
	code: Codes;
}

export function literals(value: Literals, holder: EnumHolder, a: "x" | "y", b?: "x" | "y", c?: true): string {
	return holder.code > 0 ? b ?? a : value.a;
}

export function literalReturn(): "yes" {
	return "yes";
}

export function localInterface(): number {
	interface Local {
		value: number;
	}
	const readLocal = (value: Local): number => value.value;
	return readLocal({ value: 1 });
}

export type Pair = LuaTuple<[number, string]>;

export interface PairHolder {
	pair: Pair;
}

export function pairHolders(holder: PairHolder): number {
	return holder.pair[0];
}

export function pairAlias(): Pair {
	return getPair();
}
