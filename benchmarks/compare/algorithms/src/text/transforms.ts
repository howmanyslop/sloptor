import { codes } from "./matching";

export function runLengthEncode(text: string): string {
	let output = "";
	let index = 1;
	while (index <= text.size()) {
		const character = text.sub(index, index);
		let count = 1;
		while (index + count <= text.size() && text.sub(index + count, index + count) === character) count++;
		output += `${count}${character}`;
		index += count;
	}
	return output;
}

export function runLengthDecode(encoded: string): string {
	let output = "";
	for (const [count, character] of encoded.gmatch("(%d+)(%D)")) {
		output += string.rep(character as string, tonumber(count) ?? 0);
	}
	return output;
}

export function caesar(text: string, shift: number): string {
	const result = new Array<string>();
	for (const code of codes(text)) {
		if (code >= 97 && code <= 122) result.push(string.char(((code - 97 + shift + 26 * 10) % 26) + 97));
		else if (code >= 65 && code <= 90) result.push(string.char(((code - 65 + shift + 26 * 10) % 26) + 65));
		else result.push(string.char(code));
	}
	return result.join("");
}

export function wordFrequencies(text: string): Map<string, number> {
	const counts = new Map<string, number>();
	for (const [word] of text.lower().gmatch("%a+")) {
		const key = word as string;
		counts.set(key, (counts.get(key) ?? 0) + 1);
	}
	return counts;
}

export function topWords(text: string, limit: number): Array<[string, number]> {
	const entries = new Array<[string, number]>();
	for (const [word, count] of wordFrequencies(text)) entries.push([word, count]);
	entries.sort((left, right) => (left[1] === right[1] ? left[0] < right[0] : left[1] > right[1]));
	const result = new Array<[string, number]>();
	for (let index = 0; index < math.min(limit, entries.size()); index++) result.push(entries[index]);
	return result;
}

export function isAnagram(left: string, right: string): boolean {
	const counts = new Map<number, number>();
	for (const code of codes(left.lower())) counts.set(code, (counts.get(code) ?? 0) + 1);
	for (const code of codes(right.lower())) {
		const remaining = (counts.get(code) ?? 0) - 1;
		if (remaining < 0) return false;
		counts.set(code, remaining);
	}
	for (const [, count] of counts) if (count !== 0) return false;
	return true;
}

export function suffixArray(text: string): Array<number> {
	const size = text.size();
	const suffixes = new Array<number>();
	for (let index = 0; index < size; index++) suffixes.push(index);
	let rank = codes(text);
	for (let length = 1; length < size; length *= 2) {
		const current = rank;
		const keyOf = (index: number): [number, number] => [current[index], index + length < size ? current[index + length] : -1];
		suffixes.sort((left, right) => {
			const a = keyOf(left);
			const b = keyOf(right);
			return a[0] !== b[0] ? a[0] < b[0] : a[1] < b[1];
		});
		const following = new Array<number>();
		for (let index = 0; index < size; index++) following.push(0);
		for (let index = 1; index < size; index++) {
			const a = keyOf(suffixes[index - 1]);
			const b = keyOf(suffixes[index]);
			following[suffixes[index]] = following[suffixes[index - 1]] + (a[0] !== b[0] || a[1] !== b[1] ? 1 : 0);
		}
		rank = following;
		if (following[suffixes[size - 1]] === size - 1) break;
	}
	return suffixes;
}

export function wrapText(text: string, width: number): Array<string> {
	const lines = new Array<string>();
	let line = "";
	for (const [word] of text.gmatch("%S+")) {
		const piece = word as string;
		if (line.size() === 0) line = piece;
		else if (line.size() + 1 + piece.size() <= width) line += ` ${piece}`;
		else {
			lines.push(line);
			line = piece;
		}
	}
	if (line.size() > 0) lines.push(line);
	return lines;
}
