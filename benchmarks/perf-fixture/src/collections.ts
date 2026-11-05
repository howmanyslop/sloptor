export function unique(values: number[]): number[] {
	const seen: { [key: number]: boolean } = {};
	const result: number[] = [];
	for (let i = 0; i < values.length; i++) {
		const value = values[i];
		if (!seen[value]) {
			seen[value] = true;
			result.push(value);
		}
	}
	return result;
}

export function chunk<T>(values: T[], size: number): T[][] {
	const result: T[][] = [];
	for (let i = 0; i < values.length; i += size) {
		const part: T[] = [];
		for (let j = i; j < i + size && j < values.length; j++) {
			part.push(values[j]);
		}
		result.push(part);
	}
	return result;
}

export function zip<A, B>(left: A[], right: B[]): Array<{ first: A; second: B }> {
	// biome-ignore lint/nursery/useMathMinMax: not defined
	const length = left.length < right.length ? left.length : right.length;
	const result: Array<{ first: A; second: B }> = [];
	for (let i = 0; i < length; i++) {
		result.push({ first: left[i], second: right[i] });
	}
	return result;
}
