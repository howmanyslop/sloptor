const entries: Array<Array<number | undefined>> = [[1, undefined], [undefined, 4]];
let total = 0;
for (const [a = 10, b = 20] of entries) {
	total += a + b;
}
const objs: Array<{ x?: number }> = [{ x: 1 }, {}];
for (const { x = 5 } of objs) {
	total += x;
}
print(total);
