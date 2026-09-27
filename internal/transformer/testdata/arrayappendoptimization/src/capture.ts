declare const source: ReadonlyArray<number>;

function appendFromHelper(value: number) {
	result.push(value + 100);
}

const result = new Array<number>();
for (const value of source) {
	appendFromHelper(value);
	result.push(value);
}
print(result);

const later = new Array<number>();
for (const value of source) {
	appendFromLaterHelper(value);
	later.push(value);
}

function appendFromLaterHelper(value: number) {
	later.push(value + 200);
}

print(later);
