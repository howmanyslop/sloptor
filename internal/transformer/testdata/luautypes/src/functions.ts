// Signatures: parameters, generics, `this`, rest, returns and LuaTuple packs.
export declare function plain(a: number, b: string): boolean;
export declare function optionalParams(a: number, b?: string, c?: boolean): void;
export declare function restParams(first: number, ...rest: string[]): number;
export declare function genericFn<T>(value: T): T;
export declare function constrained<T extends { id: number }>(value: T): T;
export declare function thisParam(this: { x: number }, n: number): number;
export declare function returnsUndefined(): undefined;
export declare function returnsOptional(): number | undefined;
export declare function returnsFn(): (n: number) => (s: string) => boolean;
export declare function returnsUnion(): number[] | string[];
export declare function voidCallback(cb: () => void): void;
export declare function describe(value: unknown): never;
export declare function keyword(until: number): void;

export declare function luaTuple(): LuaTuple<[number, string]>;
export declare function optionalTuple(): LuaTuple<[number, string?]>;
export declare function restTuple(): LuaTuple<[number, ...string[]]>;
export declare function emptyTuple(): LuaTuple<[]>;
export declare function maybeTuple(): LuaTuple<[number, string]> | undefined;
export declare function looseTuple(): LuaTuple<string[]>;
export declare function tupleParam(pair: LuaTuple<[number, string]>): void;
export declare function tupleCallback(cb: () => LuaTuple<[number, string]>): void;

export declare function genericCallback(identify: <T>(value: T) => T): void;
export declare function outerGeneric<K>(key: K): (other: K) => void;
export declare function keys<T>(value: T): keyof T;
export declare function indexed<T, K extends keyof T>(value: T, key: K): T[K];
export declare function restGeneric<A extends unknown[]>(...args: A): void;
export declare function tupleRest(...args: [number, string]): void;

export declare function overloaded(n: number): string;
export declare function overloaded(s: string): number;

export declare function promised(): Promise<number>;
