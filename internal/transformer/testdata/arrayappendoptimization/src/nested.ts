declare const source: ReadonlyArray<number>;

const result = new Array<number>();
for (const value of source) {
	if (value > 0) {
		result.push(value);
	} else {
		result.push(-value);
	}
	for (const inner of source) {
		result.push(inner);
	}
}
print(result);
