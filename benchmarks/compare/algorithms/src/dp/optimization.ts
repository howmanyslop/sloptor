export interface Item {
	readonly name: string;
	readonly weight: number;
	readonly value: number;
}

export function knapsack(items: ReadonlyArray<Item>, capacity: number): [number, Array<Item>] {
	const best = new Array<Array<number>>();
	for (let index = 0; index <= items.size(); index++) {
		const row = new Array<number>();
		for (let weight = 0; weight <= capacity; weight++) row.push(0);
		best.push(row);
	}
	for (let index = 1; index <= items.size(); index++) {
		const item = items[index - 1];
		for (let weight = 0; weight <= capacity; weight++) {
			best[index][weight] = best[index - 1][weight];
			if (item.weight <= weight) {
				best[index][weight] = math.max(best[index][weight], best[index - 1][weight - item.weight] + item.value);
			}
		}
	}
	const chosen = new Array<Item>();
	let remaining = capacity;
	for (let index = items.size(); index > 0; index--) {
		if (best[index][remaining] !== best[index - 1][remaining]) {
			chosen.push(items[index - 1]);
			remaining -= items[index - 1].weight;
		}
	}
	return [best[items.size()][capacity], chosen];
}

export function coinChangeWays(coins: ReadonlyArray<number>, amount: number): number {
	const ways = [1];
	for (let total = 1; total <= amount; total++) ways.push(0);
	for (const coin of coins) {
		for (let total = coin; total <= amount; total++) ways[total] += ways[total - coin];
	}
	return ways[amount];
}

export function minimumCoins(coins: ReadonlyArray<number>, amount: number): number | undefined {
	const fewest = [0];
	for (let total = 1; total <= amount; total++) {
		let best = math.huge;
		for (const coin of coins) {
			if (coin <= total && fewest[total - coin] + 1 < best) best = fewest[total - coin] + 1;
		}
		fewest.push(best);
	}
	return fewest[amount] === math.huge ? undefined : fewest[amount];
}

export function matrixChainOrder(dimensions: ReadonlyArray<number>): number {
	const count = dimensions.size() - 1;
	const cost = new Map<string, number>();
	const key = (i: number, j: number) => `${i}:${j}`;
	for (let i = 0; i < count; i++) cost.set(key(i, i), 0);
	for (let length = 2; length <= count; length++) {
		for (let i = 0; i + length - 1 < count; i++) {
			const j = i + length - 1;
			let best = math.huge;
			for (let split = i; split < j; split++) {
				const candidate =
					cost.get(key(i, split))! +
					cost.get(key(split + 1, j))! +
					dimensions[i] * dimensions[split + 1] * dimensions[j + 1];
				if (candidate < best) best = candidate;
			}
			cost.set(key(i, j), best);
		}
	}
	return cost.get(key(0, count - 1)) ?? 0;
}

export function memoize<A extends defined, R extends defined>(compute: (argument: A, recurse: (argument: A) => R) => R): (argument: A) => R {
	const cache = new Map<A, R>();
	const run = (argument: A): R => {
		const cached = cache.get(argument);
		if (cached !== undefined) return cached;
		const result = compute(argument, run);
		cache.set(argument, result);
		return result;
	};
	return run;
}

export const fibonacci = memoize<number, number>((n, recurse) => (n < 2 ? n : recurse(n - 1) + recurse(n - 2)));

export const partitionCount = memoize<number, number>((n, recurse) => {
	if (n < 0) return 0;
	if (n === 0) return 1;
	let total = 0;
	for (let k = 1; ; k++) {
		const first = (k * (3 * k - 1)) / 2;
		if (first > n) break;
		const second = (k * (3 * k + 1)) / 2;
		const sign = k % 2 === 1 ? 1 : -1;
		total += sign * (recurse(n - first) + recurse(n - second));
	}
	return total;
});

export function rodCutting(prices: ReadonlyArray<number>, length: number): number {
	const best = [0];
	for (let size = 1; size <= length; size++) {
		let value = -math.huge;
		for (let cut = 1; cut <= math.min(size, prices.size()); cut++) {
			value = math.max(value, prices[cut - 1] + best[size - cut]);
		}
		best.push(value);
	}
	return best[length];
}
