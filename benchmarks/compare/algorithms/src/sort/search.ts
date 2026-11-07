import { Comparator } from "./compare";

export function binarySearch<T extends defined>(array: ReadonlyArray<T>, target: T, comparator: Comparator<T>): number {
	let low = 0;
	let high = array.size() - 1;
	while (low <= high) {
		const middle = math.floor((low + high) / 2);
		const result = comparator(array[middle], target);
		if (result === 0) return middle;
		if (result < 0) low = middle + 1;
		else high = middle - 1;
	}
	return -1;
}

export function lowerBound<T extends defined>(array: ReadonlyArray<T>, target: T, comparator: Comparator<T>): number {
	let low = 0;
	let high = array.size();
	while (low < high) {
		const middle = math.floor((low + high) / 2);
		if (comparator(array[middle], target) < 0) low = middle + 1;
		else high = middle;
	}
	return low;
}

export function upperBound<T extends defined>(array: ReadonlyArray<T>, target: T, comparator: Comparator<T>): number {
	let low = 0;
	let high = array.size();
	while (low < high) {
		const middle = math.floor((low + high) / 2);
		if (comparator(array[middle], target) <= 0) low = middle + 1;
		else high = middle;
	}
	return low;
}

export function exponentialSearch(array: ReadonlyArray<number>, target: number): number {
	if (array.size() === 0) return -1;
	if (array[0] === target) return 0;
	let bound = 1;
	while (bound < array.size() && array[bound] <= target) bound *= 2;
	let low = math.floor(bound / 2);
	let high = math.min(bound, array.size() - 1);
	while (low <= high) {
		const middle = math.floor((low + high) / 2);
		if (array[middle] === target) return middle;
		if (array[middle] < target) low = middle + 1;
		else high = middle - 1;
	}
	return -1;
}

export function ternaryMaximum(evaluate: (x: number) => number, low: number, high: number, iterations = 80): number {
	let left = low;
	let right = high;
	for (let step = 0; step < iterations; step++) {
		const first = left + (right - left) / 3;
		const second = right - (right - left) / 3;
		if (evaluate(first) < evaluate(second)) left = first;
		else right = second;
	}
	return (left + right) / 2;
}

export function bisectRoot(evaluate: (x: number) => number, low: number, high: number, tolerance = 1e-9): number {
	let left = low;
	let right = high;
	while (right - left > tolerance) {
		const middle = (left + right) / 2;
		if (evaluate(left) * evaluate(middle) <= 0) right = middle;
		else left = middle;
	}
	return (left + right) / 2;
}

export function twoSum(sorted: ReadonlyArray<number>, target: number): [number, number] | undefined {
	let left = 0;
	let right = sorted.size() - 1;
	while (left < right) {
		const sum = sorted[left] + sorted[right];
		if (sum === target) return [left, right];
		if (sum < target) left++;
		else right--;
	}
	return undefined;
}
