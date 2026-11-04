const set = new Set<number>([1]);
const cleared = set.clear();
print(cleared);
const fe = set.forEach((v) => print(v));
print(fe);
const wm = new WeakMap<{ id: number }, number>();
const t = { id: 1 };
const deleted = wm.delete(t);
print(deleted);
const m = new Map<string, number>();
const msize = m.set("a", 1).set("b", 2).size();
print(msize);
function getMap2(): Map<string, number> {
	return new Map();
}
const chained2 = getMap2().set("k", 1);
print(chained2);
export {};
