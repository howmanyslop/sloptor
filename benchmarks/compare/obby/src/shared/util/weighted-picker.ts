import type { SeededRandom } from "./random";

export interface WeightedEntry<T> {
	readonly value: T;
	readonly weight: number;
}

/** Cumulative-weight sampler with binary search lookups. */
export class WeightedPicker<T extends defined> {
	private readonly entries: Array<WeightedEntry<T>>;
	private cumulative = new Array<number>();
	private total = 0;

	public constructor(entries: ReadonlyArray<WeightedEntry<T>> = []) {
		this.entries = entries.filter((entry) => entry.weight > 0);
		this.rebuild();
	}

	public static uniform<T extends defined>(values: ReadonlyArray<T>): WeightedPicker<T> {
		return new WeightedPicker(values.map((value) => ({ value, weight: 1 })));
	}

	public add(value: T, weight: number): this {
		if (weight > 0) {
			this.entries.push({ value, weight });
			this.rebuild();
		}
		return this;
	}

	public scale(predicate: (value: T) => boolean, factor: number): WeightedPicker<T> {
		return new WeightedPicker(
			this.entries.map(({ value, weight }) => ({ value, weight: predicate(value) ? weight * factor : weight })),
		);
	}

	public without(value: T): WeightedPicker<T> {
		return new WeightedPicker(this.entries.filter((entry) => entry.value !== value));
	}

	public getTotalWeight(): number {
		return this.total;
	}

	public size(): number {
		return this.entries.size();
	}

	public probabilityOf(value: T): number {
		if (this.total <= 0) {
			return 0;
		}
		const weight = this.entries
			.filter((entry) => entry.value === value)
			.reduce((sum, entry) => sum + entry.weight, 0);
		return weight / this.total;
	}

	public pick(random: SeededRandom): T | undefined {
		if (this.total <= 0) {
			return undefined;
		}
		const target = random.nextFloat() * this.total;
		let low = 0;
		let high = this.cumulative.size() - 1;
		while (low < high) {
			const middle = math.floor((low + high) / 2);
			if (this.cumulative[middle] <= target) {
				low = middle + 1;
			} else {
				high = middle;
			}
		}
		return this.entries[low]?.value;
	}

	public pickMany(random: SeededRandom, count: number, unique = false): Array<T> {
		const picked = new Array<T>();
		let pool: WeightedPicker<T> = this;
		for (let index = 0; index < count; index++) {
			const value = pool.pick(random);
			if (value === undefined) {
				break;
			}
			picked.push(value);
			if (unique) {
				pool = pool.without(value);
			}
		}
		return picked;
	}

	private rebuild(): void {
		let running = 0;
		this.cumulative = this.entries.map((entry) => {
			running += entry.weight;
			return running;
		});
		this.total = running;
	}
}
