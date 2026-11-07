export function codes(text: string): Array<number> {
	const result = new Array<number>();
	for (let index = 1; index <= text.size(); index++) result.push(string.byte(text, index)[0]);
	return result;
}

export function prefixFunction(pattern: ReadonlyArray<number>): Array<number> {
	const failure = [0];
	let matched = 0;
	for (let index = 1; index < pattern.size(); index++) {
		while (matched > 0 && pattern[index] !== pattern[matched]) matched = failure[matched - 1];
		if (pattern[index] === pattern[matched]) matched++;
		failure.push(matched);
	}
	return failure;
}

export function knuthMorrisPratt(text: string, pattern: string): Array<number> {
	const haystack = codes(text);
	const needle = codes(pattern);
	if (needle.size() === 0) return [];
	const failure = prefixFunction(needle);
	const matches = new Array<number>();
	let matched = 0;
	for (let index = 0; index < haystack.size(); index++) {
		while (matched > 0 && haystack[index] !== needle[matched]) matched = failure[matched - 1];
		if (haystack[index] === needle[matched]) matched++;
		if (matched === needle.size()) {
			matches.push(index - matched + 1);
			matched = failure[matched - 1];
		}
	}
	return matches;
}

export function zFunction(text: string): Array<number> {
	const values = codes(text);
	const size = values.size();
	const z = new Array<number>();
	for (let index = 0; index < size; index++) z.push(0);
	let left = 0;
	let right = 0;
	for (let index = 1; index < size; index++) {
		if (index < right) z[index] = math.min(right - index, z[index - left]);
		while (index + z[index] < size && values[z[index]] === values[index + z[index]]) z[index]++;
		if (index + z[index] > right) {
			left = index;
			right = index + z[index];
		}
	}
	if (size > 0) z[0] = size;
	return z;
}

const MODULUS = 1000000007;
const BASE = 131;

export function rabinKarp(text: string, pattern: string): Array<number> {
	const haystack = codes(text);
	const needle = codes(pattern);
	const length = needle.size();
	if (length === 0 || length > haystack.size()) return [];
	let power = 1;
	for (let index = 1; index < length; index++) power = (power * BASE) % MODULUS;
	let needleHash = 0;
	let windowHash = 0;
	for (let index = 0; index < length; index++) {
		needleHash = (needleHash * BASE + needle[index]) % MODULUS;
		windowHash = (windowHash * BASE + haystack[index]) % MODULUS;
	}
	const matches = new Array<number>();
	for (let start = 0; start + length <= haystack.size(); start++) {
		if (windowHash === needleHash) {
			let same = true;
			for (let offset = 0; offset < length && same; offset++) same = haystack[start + offset] === needle[offset];
			if (same) matches.push(start);
		}
		if (start + length < haystack.size()) {
			windowHash = (windowHash - ((haystack[start] * power) % MODULUS) + MODULUS) % MODULUS;
			windowHash = (windowHash * BASE + haystack[start + length]) % MODULUS;
		}
	}
	return matches;
}

export function boyerMooreHorspool(text: string, pattern: string): Array<number> {
	const haystack = codes(text);
	const needle = codes(pattern);
	const length = needle.size();
	if (length === 0) return [];
	const shift = new Map<number, number>();
	for (let index = 0; index < length - 1; index++) shift.set(needle[index], length - 1 - index);
	const matches = new Array<number>();
	let position = 0;
	while (position + length <= haystack.size()) {
		let offset = length - 1;
		while (offset >= 0 && needle[offset] === haystack[position + offset]) offset--;
		if (offset < 0) matches.push(position);
		position += shift.get(haystack[position + length - 1]) ?? length;
	}
	return matches;
}

export function wildcardMatch(text: string, pattern: string): boolean {
	const t = codes(text);
	const p = codes(pattern);
	const star = string.byte("*")[0];
	const question = string.byte("?")[0];
	let ti = 0;
	let pi = 0;
	let starIndex = -1;
	let resume = 0;
	while (ti < t.size()) {
		if (pi < p.size() && (p[pi] === question || p[pi] === t[ti])) {
			ti++;
			pi++;
		} else if (pi < p.size() && p[pi] === star) {
			starIndex = pi++;
			resume = ti;
		} else if (starIndex >= 0) {
			pi = starIndex + 1;
			ti = ++resume;
		} else return false;
	}
	while (pi < p.size() && p[pi] === star) pi++;
	return pi === p.size();
}
