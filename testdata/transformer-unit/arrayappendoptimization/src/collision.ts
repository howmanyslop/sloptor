declare const source: ReadonlyArray<number>;

const _resultLength = 7;
const result = new Array<number>();
for (const value of source) {
	result.push(value);
}
print(_resultLength, result);
