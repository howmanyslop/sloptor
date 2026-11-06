function makeTable(rows: number, columns: number, fill: number): Array<Array<number>> {
	const grid = new Array<Array<number>>();
	for (let row = 0; row < rows; row++) {
		const line = new Array<number>();
		for (let column = 0; column < columns; column++) line.push(fill);
		grid.push(line);
	}
	return grid;
}

function characters(text: string): Array<string> {
	const result = new Array<string>();
	for (let index = 1; index <= text.size(); index++) result.push(text.sub(index, index));
	return result;
}

export function longestCommonSubsequence(left: string, right: string): string {
	const a = characters(left);
	const b = characters(right);
	const grid = makeTable(a.size() + 1, b.size() + 1, 0);
	for (let i = 1; i <= a.size(); i++) {
		for (let j = 1; j <= b.size(); j++) {
			grid[i][j] = a[i - 1] === b[j - 1] ? grid[i - 1][j - 1] + 1 : math.max(grid[i - 1][j], grid[i][j - 1]);
		}
	}
	const output = new Array<string>();
	let i = a.size();
	let j = b.size();
	while (i > 0 && j > 0) {
		if (a[i - 1] === b[j - 1]) {
			output.unshift(a[i - 1]);
			i--;
			j--;
		} else if (grid[i - 1][j] >= grid[i][j - 1]) i--;
		else j--;
	}
	return output.join("");
}

export function editDistance(left: string, right: string): number {
	const a = characters(left);
	const b = characters(right);
	let previous = new Array<number>();
	for (let j = 0; j <= b.size(); j++) previous.push(j);
	for (let i = 1; i <= a.size(); i++) {
		const current = [i];
		for (let j = 1; j <= b.size(); j++) {
			const substitution = previous[j - 1] + (a[i - 1] === b[j - 1] ? 0 : 1);
			current.push(math.min(substitution, previous[j] + 1, current[j - 1] + 1));
		}
		previous = current;
	}
	return previous[b.size()];
}

export function longestIncreasingSubsequence(values: ReadonlyArray<number>): Array<number> {
	const tails = new Array<number>();
	const tailIndex = new Array<number>();
	const parent = new Array<number>();
	values.forEach((value, index) => {
		let low = 0;
		let high = tails.size();
		while (low < high) {
			const middle = math.floor((low + high) / 2);
			if (tails[middle] < value) low = middle + 1;
			else high = middle;
		}
		parent.push(low > 0 ? tailIndex[low - 1] : -1);
		if (low === tails.size()) {
			tails.push(value);
			tailIndex.push(index);
		} else {
			tails[low] = value;
			tailIndex[low] = index;
		}
	});
	const result = new Array<number>();
	let cursor = tailIndex.size() > 0 ? tailIndex[tailIndex.size() - 1] : -1;
	while (cursor >= 0) {
		result.unshift(values[cursor]);
		cursor = parent[cursor];
	}
	return result;
}

export function maximumSubarray(values: ReadonlyArray<number>): [number, number, number] {
	let best = -math.huge;
	let bestStart = 0;
	let bestEnd = 0;
	let running = 0;
	let start = 0;
	values.forEach((value, index) => {
		if (running <= 0) {
			running = value;
			start = index;
		} else running += value;
		if (running > best) {
			best = running;
			bestStart = start;
			bestEnd = index;
		}
	});
	return [best, bestStart, bestEnd];
}

export function longestPalindrome(text: string): string {
	const chars = characters(text);
	let bestStart = 0;
	let bestLength = chars.size() > 0 ? 1 : 0;
	const expand = (left: number, right: number) => {
		while (left >= 0 && right < chars.size() && chars[left] === chars[right]) {
			left--;
			right++;
		}
		if (right - left - 1 > bestLength) {
			bestLength = right - left - 1;
			bestStart = left + 1;
		}
	};
	for (let center = 0; center < chars.size(); center++) {
		expand(center, center);
		expand(center, center + 1);
	}
	return text.sub(bestStart + 1, bestStart + bestLength);
}
