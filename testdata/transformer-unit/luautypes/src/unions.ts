// Unions, optionality, intersections and overload sets.
export declare const maybeNumber: number | undefined;
export declare const nullableString: string | null;
export declare const numberOrString: number | string;
export declare const optionalUnion: number | string | undefined;
export declare const boolPair: true | false;
export declare const boolOrUndefined: boolean | undefined;
export declare const literals: "a" | "b";
export declare const numberLiterals: 1 | 2;
export declare const mixedLiterals: "a" | 1;
export declare const fnOrNil: ((n: number) => string) | undefined;
export declare const fnOrString: ((n: number) => string) | string;
export declare const tableUnion: { a: number } | { b: string };
export declare const arrayUnion: number[] | string[];
export declare const dupes: Array<number> | number[];
export declare const onlyNil: null | undefined;

export declare const tableIntersection: { a: number } & { b: string };
export declare const emptyIntersection: { a: number } & {};
export declare const brandedString: string & { __brand: "id" };
export declare const overloads: { (n: number): string; (s: string): number };

export interface Overloaded {
	(value: number): string;
	(value: string): number;
}

export declare const overloadedAlias: Overloaded;
