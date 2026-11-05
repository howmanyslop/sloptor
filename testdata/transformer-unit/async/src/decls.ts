async function fetchValue(x: number): Promise<number> {
	const a = await Promise.resolve(x);
	const b = await fetchValue(a);
	return a + b;
}
const asyncArrow = async (n: number) => n * 2;
export async function exported(): Promise<void> {
	await asyncArrow(1);
}
function* gen(): Generator<number, string, undefined> {
	yield 1;
	yield 2;
	return "done";
}
function* gen2() {
	yield;
	const v = yield* gen();
	print(v);
}
export function useGen() {
	for (const x of gen2()) {
		print(x);
	}
}
