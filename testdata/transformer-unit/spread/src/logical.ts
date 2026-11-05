let v: number | undefined;
v ??= 1;
const holder: { p?: number } = {};
function f() {
	return 5;
}
holder.p ??= f();
const arrIdx: Array<number | undefined> = [];
let i = 0;
arrIdx[i] ??= f();
function get(): { p?: number } {
	return holder;
}
get().p ??= f();
let s: string | undefined = "x";
s &&= "y";
s ||= "z";
let n = 0;
n ||= 7;
n &&= 8;
const useAsExpr = (v ??= 2);
print(v, holder.p, arrIdx[i], s, n, useAsExpr, i);
