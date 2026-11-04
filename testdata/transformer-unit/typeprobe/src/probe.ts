// Type-probe declarations for the type-predicate and truthiness tests.
// `declare let` carries the annotated type to the declaration-name identifier
// with no initializer (so no control-flow narrowing and no unassigned-use
// errors under strict).

declare let num: number;
declare let str: string;
declare let five: 5;
declare let zeroOne: 0 | 1;
declare let strOpt: string | undefined;
declare let bool: boolean;
declare let numStr: number | string;
declare let zero: 0;
declare let empty: "";
declare let unk: unknown;
declare let anyVal: any;
declare let troo: true;
declare let definedVal: {};
declare let branded: string & { _brand: never };

interface Base {
	kind: string;
}
interface Derived extends Base {
	extra: number;
}
declare let derived: Derived;

// Generic constraint lookup: the parameter names carry the type variables.
declare function constrainedProbe<T extends string>(conT: T): T;
declare function unconstrainedProbe<U>(unconU: U): U;

// The FRESH boolean literal types survive on `const x = true/false`
// declaration names; annotations and expression nodes carry the regular
// variants (getTypeAtLocation strips freshness from expressions).
const troof = true;
const falsef = false;
