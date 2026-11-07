export function* range(start: number, finish: number, step = 1): Generator<number> {
	if (step === 0) {
		return;
	}
	for (let value = start; step > 0 ? value < finish : value > finish; value += step) {
		yield value;
	}
}

export function* enumerate<T extends defined>(items: ReadonlyArray<T>): Generator<[index: number, item: T]> {
	let index = 0;
	for (const item of items) {
		yield [index, item];
		index += 1;
	}
}

export function* pairwise<T extends defined>(items: ReadonlyArray<T>): Generator<[previous: T, current: T]> {
	for (let index = 1; index < items.size(); index++) {
		yield [items[index - 1], items[index]];
	}
}

export function* chunk<T extends defined>(items: ReadonlyArray<T>, size: number): Generator<Array<T>> {
	let current = new Array<T>();
	for (const item of items) {
		current.push(item);
		if (current.size() >= size) {
			yield current;
			current = [];
		}
	}
	if (!current.isEmpty()) {
		yield current;
	}
}

export function collect<T>(iterator: Generator<T>): Array<NonNullable<T>> {
	const result = new Array<NonNullable<T>>();
	for (const value of iterator) {
		if (value !== undefined) {
			result.push(value as NonNullable<T>);
		}
	}
	return result;
}

export function flatMap<T extends defined, U extends defined>(items: ReadonlyArray<T>, mapper: (item: T, index: number) => ReadonlyArray<U>): Array<U> {
	const result = new Array<U>();
	items.forEach((item, index) => {
		for (const mapped of mapper(item, index)) {
			result.push(mapped);
		}
	});
	return result;
}

export function groupBy<T extends defined, K extends defined>(items: ReadonlyArray<T>, keyOf: (item: T) => K): Map<K, Array<T>> {
	const groups = new Map<K, Array<T>>();
	for (const item of items) {
		const key = keyOf(item);
		const bucket = groups.get(key);
		if (bucket) {
			bucket.push(item);
		} else {
			groups.set(key, [item]);
		}
	}
	return groups;
}

export function countBy<T extends defined, K extends defined>(items: ReadonlyArray<T>, keyOf: (item: T) => K): Map<K, number> {
	const counts = new Map<K, number>();
	for (const item of items) {
		const key = keyOf(item);
		counts.set(key, (counts.get(key) ?? 0) + 1);
	}
	return counts;
}

export function maxBy<T extends defined>(items: ReadonlyArray<T>, score: (item: T) => number): T | undefined {
	let best: T | undefined;
	let bestScore = -math.huge;
	for (const item of items) {
		const value = score(item);
		if (value > bestScore) {
			best = item;
			bestScore = value;
		}
	}
	return best;
}

export function uniqueBy<T extends defined, K>(items: ReadonlyArray<T>, keyOf: (item: T) => K): Array<T> {
	const seen = new Set<K>();
	return items.filter((item) => {
		const key = keyOf(item);
		if (seen.has(key)) {
			return false;
		}
		seen.add(key);
		return true;
	});
}
