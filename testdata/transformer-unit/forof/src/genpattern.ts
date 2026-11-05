declare const pairsGen: Generator<[number, number], void, unknown>;
for (const [a, b] of pairsGen) {
	print(a, b);
}
declare const objSet: Set<{ x: number }>;
for (const { x } of objSet) {
	print(x);
}