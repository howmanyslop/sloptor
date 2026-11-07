export class MultiMap<K extends defined, V extends defined> {
	private readonly buckets = new Map<K, Array<V>>();
	private total = 0;

	public put(key: K, value: V): void {
		let bucket = this.buckets.get(key);
		if (!bucket) {
			bucket = [];
			this.buckets.set(key, bucket);
		}
		bucket.push(value);
		this.total++;
	}

	public get(key: K): ReadonlyArray<V> {
		return this.buckets.get(key) ?? [];
	}

	public removeValue(key: K, value: V): boolean {
		const bucket = this.buckets.get(key);
		if (!bucket) return false;
		const index = bucket.indexOf(value);
		if (index < 0) return false;
		bucket.remove(index);
		this.total--;
		if (bucket.size() === 0) this.buckets.delete(key);
		return true;
	}

	public removeAll(key: K): number {
		const removed = this.buckets.get(key)?.size() ?? 0;
		this.buckets.delete(key);
		this.total -= removed;
		return removed;
	}

	public keyCount(): number {
		return this.buckets.size();
	}

	public size(): number {
		return this.total;
	}

	public *entries(): Generator<[K, V]> {
		for (const [key, bucket] of this.buckets) for (const value of bucket) yield [key, value];
	}

	public invert(): MultiMap<V, K> {
		const inverted = new MultiMap<V, K>();
		for (const [key, value] of this.entries()) inverted.put(value, key);
		return inverted;
	}

	public static groupBy<T extends defined, K extends defined>(values: ReadonlyArray<T>, selector: (value: T) => K): MultiMap<K, T> {
		const result = new MultiMap<K, T>();
		for (const value of values) result.put(selector(value), value);
		return result;
	}
}

export class BiMap<K extends defined, V extends defined> {
	private readonly forwardMap = new Map<K, V>();
	private readonly inverseMap = new Map<V, K>();

	public set(key: K, value: V): void {
		const oldValue = this.forwardMap.get(key);
		if (oldValue !== undefined) this.inverseMap.delete(oldValue);
		const oldKey = this.inverseMap.get(value);
		if (oldKey !== undefined) this.forwardMap.delete(oldKey);
		this.forwardMap.set(key, value);
		this.inverseMap.set(value, key);
	}

	public getValue(key: K): V | undefined {
		return this.forwardMap.get(key);
	}

	public getKey(value: V): K | undefined {
		return this.inverseMap.get(value);
	}

	public size(): number {
		return this.forwardMap.size();
	}
}

export class CountingSet<T extends defined> {
	private readonly counts = new Map<T, number>();

	public add(value: T, amount = 1): number {
		const updated = (this.counts.get(value) ?? 0) + amount;
		if (updated <= 0) this.counts.delete(value);
		else this.counts.set(value, updated);
		return math.max(updated, 0);
	}

	public count(value: T): number {
		return this.counts.get(value) ?? 0;
	}

	public mostCommon(limit: number): Array<[T, number]> {
		const entries = new Array<[T, number]>();
		for (const entry of this.counts) entries.push(entry);
		entries.sort((left, right) => left[1] > right[1]);
		const result = new Array<[T, number]>();
		for (let index = 0; index < math.min(limit, entries.size()); index++) result.push(entries[index]);
		return result;
	}
}
