// oxlint-disable unicorn-js/no-for-loop typescript/prefer-for-of typescript/consistent-indexed-object-style -- no

export function unique(values: Array<number>): Array<number> {
	const seen: { [key: number]: boolean } = {};
	const result: Array<number> = [];
	for (let index = 0; index < values.length; index += 1) {
		const value = values[index];
		if (!seen[value]) {
			seen[value] = true;
			result.push(value);
		}
	}
	return result;
}

export function chunk<TValue>(values: Array<TValue>, size: number): Array<Array<TValue>> {
	const result: Array<Array<TValue>> = [];
	for (let index = 0; index < values.length; index += size) {
		const part: Array<TValue> = [];
		for (let jndex = index; jndex < index + size && jndex < values.length; jndex += 1) {
			part.push(values[jndex]);
		}
		result.push(part);
	}
	return result;
}

export function zip<TLeft, TRight>(left: Array<TLeft>, right: Array<TRight>): Array<{ first: TLeft; second: TRight }> {
	// biome-ignore lint/complexity/useMathMinMax: no
	const length = left.length < right.length ? left.length : right.length;
	const result: Array<{ first: TLeft; second: TRight }> = [];
	for (let index = 0; index < length; index += 1) {
		result.push({ first: left[index], second: right[index] });
	}
	return result;
}
