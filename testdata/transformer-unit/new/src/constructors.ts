const a = new Array<number>();
const b = new Array<number>(8);
const c = new Array<string>(4, "x");
const s1 = new Set(["a", "b"]);
const s2 = new Set<string>();
const src = ["x", "y"];
const s3 = new Set(src);
let i = 0;
const s4 = new Set([i++, i]);
const m1 = new Map([
	["a", 1],
	["b", 2],
]);
const m2 = new Map<string, number>();
const entries: ReadonlyArray<[string, number]> = [["k", 3]];
const m3 = new Map(entries);
const m4 = new Map<string, number>([]);
function makePairs(): Array<[string, number]> {
	return [["p", 9]];
}
const m5 = new Map(makePairs());
const wm = new WeakMap<{ id: number }, number>();
const ws = new WeakSet<{ id: number }>();
const rm: ReadonlyMap<string, number> = new ReadonlyMap([["a", 1]]);
const rs: ReadonlySet<string> = new ReadonlySet(["q"]);
const part = new Instance("Part");
interface Thing {
	readonly val: number;
}
interface ThingConstructor {
	new (val: number): Thing;
}
declare const Thing: ThingConstructor;
const t = new Thing(5);
print(a, b, c, s1, s2, s3, s4, m1, m2, m3, m4, m5, wm, ws, rm, rs, part, t);
