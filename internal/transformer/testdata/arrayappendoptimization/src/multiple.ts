declare const source: ReadonlyArray<number>;
declare function nextValue(label: string): number;

const result = new Array<number>();
for (const value of source) {
	const length = result.push(nextValue("first"), value, nextValue("second"));
	if (value > 0) {
		result.push(value);
	}
	const unchanged = result.push();
	print(length, unchanged);
}
print(result);
