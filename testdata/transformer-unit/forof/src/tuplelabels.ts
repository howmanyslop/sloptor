declare const it1: IterableFunction<LuaTuple<[end: number, value: string]>>;
for (const item of it1) {
	print(item[0], item[1]);
}