declare const source: ReadonlyArray<number>;
declare const anySource: ReadonlyArray<any>;
declare function consume(value: unknown): void;

const nonEmpty = [1];
for (const value of source) nonEmpty.push(value);

let mutable = new Array<number>();
for (const value of source) mutable.push(value);

const separated = new Array<number>();
print("barrier");
for (const value of source) separated.push(value);

const observed = new Array<number>();
for (const value of source) {
	print(observed.size());
	observed.push(value);
}

const aliased = new Array<number>();
for (const value of source) {
	consume(aliased);
	aliased.push(value);
}

const captured = new Array<number>();
for (const value of source) {
	const inspect = () => captured.size();
	consume(inspect);
	captured.push(value);
}

const mutated = new Array<number>();
for (const value of source) {
	mutated.push(value);
	mutated.pop();
}

const indexed = new Array<number>();
for (const value of source) {
	indexed[0] = value;
	indexed.push(value);
}

const pair = [1, 2] as const;
const spread = new Array<number>();
for (const value of source) {
	spread.push(...pair);
	consume(value);
}

const anyValues = new Array<any>();
for (const value of anySource) anyValues.push(value);

const condition = new Array<number>();
for (let index = 0; index < condition.size(); index++) condition.push(index);

let incrementIndex = 0;
const increment = new Array<number>();
for (; incrementIndex < 3; increment.push(incrementIndex)) incrementIndex++;

const iterated = new Array<number>();
for (const value of iterated) iterated.push(value);

const first = new Array<number>(), second = new Array<number>();
for (const value of source) first.push(value);
consume(second);

class CustomCollection {
	public push(_value: number): number {
		return 0;
	}
}
const custom = new CustomCollection();
for (const value of source) custom.push(value);
