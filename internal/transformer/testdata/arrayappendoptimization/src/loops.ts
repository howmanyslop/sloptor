declare const source: ReadonlyArray<number>;

const fromFor = new Array<number>();
for (let index = 0; index < 4; index++) {
	if (index === 1) continue;
	fromFor.push(index);
	if (index === 2) break;
}

const fromForOf = new Array<number>();
for (const value of source) {
	fromForOf.push(value);
}

let whileIndex = 0;
const fromWhile = new Array<number>();
while (whileIndex < 3) {
	fromWhile.push(whileIndex++);
}

let doIndex = 0;
const fromDo = new Array<number>();
do {
	fromDo.push(doIndex++);
} while (doIndex < 3);

print(fromFor, fromForOf, fromWhile, fromDo);
