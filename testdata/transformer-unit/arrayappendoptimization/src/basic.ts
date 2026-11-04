declare const source: ReadonlyArray<number>;

const result = new Array<number>();
for (const value of source) {
	result.push(value);
}
print(result);
