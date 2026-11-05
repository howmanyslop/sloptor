declare const restIt: IterableFunction<LuaTuple<[string, ...number[]]>>;
for (const packed of restIt) {
	print(packed[0]);
}
declare function make(): IterableFunction<LuaTuple<[number, string]>>;
let tup: [number, string];
for (tup of make()) {
	print(tup[1]);
}