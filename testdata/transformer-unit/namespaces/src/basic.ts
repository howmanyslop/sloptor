namespace Outer {
	export const value = 1;
	export let mut = 2;
	export function fn() {
		return value;
	}
	export function bump() {
		mut += 1;
		return mut;
	}
	const hidden = 3;
	export namespace Inner {
		export const deep = hidden;
	}
}
export function useNs() {
	print(Outer.value, Outer.fn(), Outer.Inner.deep, Outer.mut, Outer.bump());
	Outer.mut = 5;
}
namespace NoExports {
	const x = 1;
	print(x);
}
namespace AB.CD {
	export const v = 1;
}
export function useAB() {
	print(AB.CD.v);
}
declare namespace AmbientNs {
	const x: number;
}
namespace TypesOnly {
	export type T = number;
}
export namespace ExpNs {
	export const e = 1;
	export const p = 1,
		q = 2;
}
