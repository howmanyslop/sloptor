declare const m: Map<string, number>;
for (const pair of m) {
	print(pair[0], pair[1]);
}
let entry: [string, number];
for (entry of m) {
	print(entry[0]);
}
declare const mOpt: Map<string, number | undefined>;
for (const [k2, v2 = 5] of mOpt) {
	print(k2, v2);
}
declare const nestedMap: Map<string, [number, number]>;
for (const [k3, [a1, b1]] of nestedMap) {
	print(k3, a1, b1);
}