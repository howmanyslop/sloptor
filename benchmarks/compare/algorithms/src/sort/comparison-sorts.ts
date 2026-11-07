import { Comparator, copyArray, swap } from "./compare";

export function insertionSort<T extends defined>(input: ReadonlyArray<T>, comparator: Comparator<T>): Array<T> {
	const array = copyArray(input);
	for (let index = 1; index < array.size(); index++) {
		const current = array[index];
		let position = index - 1;
		while (position >= 0 && comparator(array[position], current) > 0) {
			array[position + 1] = array[position];
			position--;
		}
		array[position + 1] = current;
	}
	return array;
}

export function selectionSort<T extends defined>(input: ReadonlyArray<T>, comparator: Comparator<T>): Array<T> {
	const array = copyArray(input);
	const size = array.size();
	for (let index = 0; index < size - 1; index++) {
		let smallest = index;
		for (let candidate = index + 1; candidate < size; candidate++) {
			if (comparator(array[candidate], array[smallest]) < 0) smallest = candidate;
		}
		if (smallest !== index) swap(array, index, smallest);
	}
	return array;
}

function merge<T extends defined>(left: Array<T>, right: Array<T>, comparator: Comparator<T>): Array<T> {
	const result = new Array<T>();
	let leftIndex = 0;
	let rightIndex = 0;
	while (leftIndex < left.size() && rightIndex < right.size()) {
		if (comparator(left[leftIndex], right[rightIndex]) <= 0) {
			result.push(left[leftIndex++]);
		} else {
			result.push(right[rightIndex++]);
		}
	}
	while (leftIndex < left.size()) result.push(left[leftIndex++]);
	while (rightIndex < right.size()) result.push(right[rightIndex++]);
	return result;
}

export function mergeSort<T extends defined>(input: ReadonlyArray<T>, comparator: Comparator<T>): Array<T> {
	if (input.size() <= 1) return copyArray(input);
	const middle = math.floor(input.size() / 2);
	const left = new Array<T>();
	const right = new Array<T>();
	input.forEach((value, index) => (index < middle ? left : right).push(value));
	return merge(mergeSort(left, comparator), mergeSort(right, comparator), comparator);
}

function partition<T extends defined>(array: Array<T>, low: number, high: number, comparator: Comparator<T>): number {
	const middle = math.floor((low + high) / 2);
	swap(array, middle, high);
	const pivot = array[high];
	let store = low;
	for (let index = low; index < high; index++) {
		if (comparator(array[index], pivot) < 0) {
			swap(array, index, store);
			store++;
		}
	}
	swap(array, store, high);
	return store;
}

function quickSortRange<T extends defined>(array: Array<T>, low: number, high: number, comparator: Comparator<T>): void {
	while (low < high) {
		if (high - low < 12) {
			for (let index = low + 1; index <= high; index++) {
				const current = array[index];
				let position = index - 1;
				while (position >= low && comparator(array[position], current) > 0) {
					array[position + 1] = array[position];
					position--;
				}
				array[position + 1] = current;
			}
			return;
		}
		const pivot = partition(array, low, high, comparator);
		if (pivot - low < high - pivot) {
			quickSortRange(array, low, pivot - 1, comparator);
			low = pivot + 1;
		} else {
			quickSortRange(array, pivot + 1, high, comparator);
			high = pivot - 1;
		}
	}
}

export function quickSort<T extends defined>(input: ReadonlyArray<T>, comparator: Comparator<T>): Array<T> {
	const array = copyArray(input);
	quickSortRange(array, 0, array.size() - 1, comparator);
	return array;
}

function siftDown<T extends defined>(array: Array<T>, start: number, size: number, comparator: Comparator<T>): void {
	let root = start;
	while (true) {
		const left = root * 2 + 1;
		if (left >= size) return;
		let largest = root;
		if (comparator(array[left], array[largest]) > 0) largest = left;
		const right = left + 1;
		if (right < size && comparator(array[right], array[largest]) > 0) largest = right;
		if (largest === root) return;
		swap(array, root, largest);
		root = largest;
	}
}

export function heapSort<T extends defined>(input: ReadonlyArray<T>, comparator: Comparator<T>): Array<T> {
	const array = copyArray(input);
	const size = array.size();
	for (let index = math.floor(size / 2) - 1; index >= 0; index--) siftDown(array, index, size, comparator);
	for (let finish = size - 1; finish > 0; finish--) {
		swap(array, 0, finish);
		siftDown(array, 0, finish, comparator);
	}
	return array;
}

export function shellSort<T extends defined>(input: ReadonlyArray<T>, comparator: Comparator<T>): Array<T> {
	const array = copyArray(input);
	const gaps = [701, 301, 132, 57, 23, 10, 4, 1];
	for (const gap of gaps) {
		for (let index = gap; index < array.size(); index++) {
			const current = array[index];
			let position = index;
			while (position >= gap && comparator(array[position - gap], current) > 0) {
				array[position] = array[position - gap];
				position -= gap;
			}
			array[position] = current;
		}
	}
	return array;
}
