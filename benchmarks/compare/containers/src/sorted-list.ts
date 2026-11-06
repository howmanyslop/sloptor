import { AbstractCollection, Ordering, defaultOrdering } from "./collection";

export class SortedList<T extends defined> extends AbstractCollection<T> {
	private readonly items = new Array<T>();

	public constructor(private readonly ordering: Ordering<T> = defaultOrdering) {
		super((left, right) => ordering(left, right) === 0);
	}

	public size(): number {
		return this.items.size();
	}

	public clear(): void {
		this.items.clear();
	}

	private bound(value: T, inclusive: boolean): number {
		let low = 0;
		let high = this.items.size();
		while (low < high) {
			const middle = math.floor((low + high) / 2);
			const result = this.ordering(this.items[middle], value);
			if (result < 0 || (inclusive && result === 0)) low = middle + 1;
			else high = middle;
		}
		return low;
	}

	public add(value: T): void {
		this.items.insert(this.bound(value, true), value);
	}

	public contains(value: T): boolean {
		const index = this.bound(value, false);
		return index < this.items.size() && this.ordering(this.items[index], value) === 0;
	}

	public remove(value: T): boolean {
		const index = this.bound(value, false);
		if (index < this.items.size() && this.ordering(this.items[index], value) === 0) {
			this.items.remove(index);
			return true;
		}
		return false;
	}

	public at(index: number): T | undefined {
		return this.items[index];
	}

	public rank(value: T): number {
		return this.bound(value, false);
	}

	public *between(low: T, high: T): Generator<T> {
		for (let index = this.bound(low, false); index < this.items.size(); index++) {
			if (this.ordering(this.items[index], high) > 0) return;
			yield this.items[index];
		}
	}

	public floor(value: T): T | undefined {
		const index = this.bound(value, true);
		return index > 0 ? this.items[index - 1] : undefined;
	}

	public ceiling(value: T): T | undefined {
		return this.items[this.bound(value, false)];
	}

	public *values(): Generator<T> {
		for (const item of this.items) yield item;
	}

	protected kind(): string {
		return "SortedList";
	}
}

export class Leaderboard {
	private readonly scores = new Map<string, number>();
	private readonly ordered = new SortedList<[number, string]>((left, right) =>
		left[0] !== right[0] ? right[0] - left[0] : left[1] < right[1] ? -1 : left[1] > right[1] ? 1 : 0,
	);

	public submit(player: string, score: number): void {
		const previous = this.scores.get(player);
		if (previous !== undefined) {
			if (previous >= score) return;
			this.ordered.remove([previous, player]);
		}
		this.scores.set(player, score);
		this.ordered.add([score, player]);
	}

	public top(count: number): Array<string> {
		const result = new Array<string>();
		for (const [score, player] of this.ordered.values()) {
			if (result.size() >= count) break;
			result.push(`${player}:${score}`);
		}
		return result;
	}

	public placement(player: string): number | undefined {
		const score = this.scores.get(player);
		return score === undefined ? undefined : this.ordered.rank([score, player]) + 1;
	}
}
