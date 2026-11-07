export type Comparator<T> = (left: T, right: T) => number;

export function numberAscending(left: number, right: number): number {
	return left - right;
}

export function numberDescending(left: number, right: number): number {
	return right - left;
}

export function stringAscending(left: string, right: string): number {
	if (left < right) return -1;
	if (left > right) return 1;
	return 0;
}

export function reverseComparator<T extends defined>(comparator: Comparator<T>): Comparator<T> {
	return (left, right) => comparator(right, left);
}

export function byKey<T extends defined, K extends defined>(selector: (value: T) => K, comparator: Comparator<K>): Comparator<T> {
	return (left, right) => comparator(selector(left), selector(right));
}

export function thenBy<T extends defined>(first: Comparator<T>, second: Comparator<T>): Comparator<T> {
	return (left, right) => {
		const result = first(left, right);
		return result !== 0 ? result : second(left, right);
	};
}

export function swap<T extends defined>(array: Array<T>, first: number, second: number): void {
	const temporary = array[first];
	array[first] = array[second];
	array[second] = temporary;
}

export function copyArray<T extends defined>(array: ReadonlyArray<T>): Array<T> {
	const result = new Array<T>();
	for (const value of array) result.push(value);
	return result;
}

export function isSorted<T extends defined>(array: ReadonlyArray<T>, comparator: Comparator<T>): boolean {
	for (let index = 1; index < array.size(); index++) {
		if (comparator(array[index - 1], array[index]) > 0) return false;
	}
	return true;
}

export function range(start: number, finish: number, step = 1): Array<number> {
	const result = new Array<number>();
	for (let value = start; step > 0 ? value < finish : value > finish; value += step) result.push(value);
	return result;
}

export class LinearCongruential {
	private state: number;

	public constructor(seed: number) {
		this.state = seed % 2147483647;
		if (this.state <= 0) this.state += 2147483646;
	}

	public next(): number {
		this.state = (this.state * 16807) % 2147483647;
		return this.state;
	}

	public nextFloat(): number {
		return (this.next() - 1) / 2147483646;
	}

	public nextInt(minimum: number, maximum: number): number {
		return minimum + math.floor(this.nextFloat() * (maximum - minimum + 1));
	}

	public shuffle<T extends defined>(array: Array<T>): Array<T> {
		for (let index = array.size() - 1; index > 0; index--) {
			swap(array, index, this.nextInt(0, index));
		}
		return array;
	}
}
