export function countingSort(input: ReadonlyArray<number>): Array<number> {
	if (input.size() === 0) return [];
	let minimum = input[0];
	let maximum = input[0];
	for (const value of input) {
		minimum = math.min(minimum, value);
		maximum = math.max(maximum, value);
	}
	const counts = new Map<number, number>();
	for (const value of input) counts.set(value, (counts.get(value) ?? 0) + 1);
	const result = new Array<number>();
	for (let value = minimum; value <= maximum; value++) {
		const count = counts.get(value) ?? 0;
		for (let copy = 0; copy < count; copy++) result.push(value);
	}
	return result;
}

function digitAt(value: number, place: number, base: number): number {
	return math.floor(value / place) % base;
}

export function radixSort(input: ReadonlyArray<number>, base = 10): Array<number> {
	let array = new Array<number>();
	let maximum = 0;
	for (const value of input) {
		array.push(value);
		maximum = math.max(maximum, value);
	}
	for (let place = 1; math.floor(maximum / place) > 0; place *= base) {
		const buckets = new Array<Array<number>>();
		for (let digit = 0; digit < base; digit++) buckets.push([]);
		for (const value of array) buckets[digitAt(value, place, base)].push(value);
		array = [];
		for (const bucket of buckets) for (const value of bucket) array.push(value);
	}
	return array;
}

export function bucketSort(input: ReadonlyArray<number>, bucketCount = 8): Array<number> {
	if (input.size() < 2) return [...input];
	let minimum = math.huge;
	let maximum = -math.huge;
	for (const value of input) {
		if (value < minimum) minimum = value;
		if (value > maximum) maximum = value;
	}
	const width = (maximum - minimum) / bucketCount + 1e-9;
	const buckets = new Array<Array<number>>();
	for (let index = 0; index < bucketCount; index++) buckets.push([]);
	for (const value of input) {
		const bucketIndex = math.min(bucketCount - 1, math.floor((value - minimum) / width));
		buckets[bucketIndex].push(value);
	}
	const result = new Array<number>();
	for (const bucket of buckets) {
		bucket.sort((left, right) => left < right);
		for (const value of bucket) result.push(value);
	}
	return result;
}

export function medianOfArray(input: ReadonlyArray<number>): number | undefined {
	if (input.size() === 0) return undefined;
	const sorted = countingSort(input.map((value) => math.floor(value)));
	const middle = math.floor(sorted.size() / 2);
	return sorted.size() % 2 === 1 ? sorted[middle] : (sorted[middle - 1] + sorted[middle]) / 2;
}

export function quickSelect(input: ReadonlyArray<number>, rank: number): number {
	let candidates = [...input];
	let target = rank;
	while (true) {
		const pivot = candidates[math.floor(candidates.size() / 2)];
		const lower = candidates.filter((value) => value < pivot);
		const equal = candidates.filter((value) => value === pivot);
		const higher = candidates.filter((value) => value > pivot);
		if (target < lower.size()) {
			candidates = lower;
		} else if (target < lower.size() + equal.size()) {
			return pivot;
		} else {
			target -= lower.size() + equal.size();
			candidates = higher;
		}
	}
}
