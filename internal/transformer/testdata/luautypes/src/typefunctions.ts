// Type-level aliases: a generic alias whose body is a mapped, conditional or
// template literal type becomes a Luau type function; one the translator
// cannot express becomes `any` with a loss comment.
export type Partial2<T> = { [K in keyof T]?: T[K] };

export type Required2<T> = { [K in keyof T]-?: T[K] };

export type Readonly2<T> = { readonly [K in keyof T]: T[K] };

export type Elem<T> = T extends (infer U)[] ? U : T;

export type Handlers<T extends string> = { [K in T as `on${K}`]: () => void };

export type Stringy<T> = T extends string ? "yes" : "no";

export type Returned<T> = T extends (...args: any) => infer R ? R : never;

export type Greeting<T extends string> = `hello ${T}`;

export type Keys<T> = T extends string ? never : keyof T;

export type Lookup<T, K extends keyof T> = T extends string ? never : T[K];

// A body that is only `keyof T` / `T[K]` stays an alias over Luau's built-in
// type functions.
export type Values<T> = T[keyof T];

export interface Box<V> {
	value: V;
}

// Fallbacks: an `infer` outside the two translated shapes, an `as` clause
// over another alias, a mapped value and an extends clause naming another
// type.
export type Unwrap<T> = T extends Promise<infer U> ? U : T;

export type Renamed<T> = { [K in keyof T as Uppercase<K & string>]: T[K] };

export type Boxed<T> = { [K in keyof T]: Box<T[K]> };

export type IsBox<T> = T extends Box<unknown> ? true : false;

export interface Config {
	host: string;
	port: number;
}

// Instantiated uses of complete translations keep the call-site spelling.
export function patch(config: Config, partial: Partial2<Config>): Config {
	return { host: partial.host ?? config.host, port: partial.port ?? config.port };
}

export function first(values: number[]): Elem<number[]> {
	return values[0];
}

export function handlers(): Handlers<"click" | "hover"> {
	return { onclick: () => {}, onhover: () => {} };
}

export function describe(value: Stringy<string>): string {
	return value;
}

export function useResult(value: Returned<() => number>): number {
	return value;
}

export function greet(value: Greeting<"world">): string {
	return value;
}

export function required(config: Required2<Config>, frozen: Readonly2<Config>): number {
	return config.port + frozen.port;
}

export function lookup(key: Keys<Config>, value: Lookup<Config, "port">): Values<Config> {
	return key === "host" ? value : value + 1;
}

// Instantiated uses of a fallback expand eagerly; generic uses name it.
export function box(value: Boxed<Config>): Box<string> {
	return value.host;
}

export function unwrap<T>(value: Unwrap<T>): Unwrap<T> {
	return value;
}

// Generic uses of the built-in helpers go through the prelude.
export function fill<T>(partial: Partial<T>, defaults: T): T {
	return defaults;
}

export function freeze<T>(value: T): Readonly<T> {
	return value;
}

export function needAll<T>(value: Required<T>): Required<T> {
	return value;
}

export function only<T, K extends keyof T>(value: Pick<T, K>): Pick<T, K> {
	return value;
}

export function without<T, K extends keyof T>(value: Omit<T, K>): Omit<T, K> {
	return value;
}

export function record<K extends string, V>(value: Record<K, V>): Record<K, V> {
	return value;
}

export function exclude<T, U>(value: Exclude<T, U>): Exclude<T, U> {
	return value;
}

export function extract<T, U>(value: Extract<T, U>): Extract<T, U> {
	return value;
}

export function defined<T>(value: NonNullable<T>): NonNullable<T> {
	return value;
}

export function returned<T extends (...args: any) => any>(value: ReturnType<T>): ReturnType<T> {
	return value;
}

export function parameters<T extends (...args: any) => any>(value: Parameters<T>): Parameters<T> {
	return value;
}

// An instantiated built-in helper expands eagerly without a prelude.
export function partialConfig(value: Partial<Config>): Pick<Config, "host"> {
	return { host: value.host ?? "" };
}
