interface CacheEntry<K, V> {
	key: K;
	value: V;
	expiresAt: number;
	newer?: CacheEntry<K, V>;
	older?: CacheEntry<K, V>;
}

export interface CacheStats {
	hits: number;
	misses: number;
	evictions: number;
}

export type EvictionListener<K, V> = (key: K, value: V) => void;

export class LruCache<K extends defined, V extends defined> {
	private readonly entries = new Map<K, CacheEntry<K, V>>();
	private newest?: CacheEntry<K, V>;
	private oldest?: CacheEntry<K, V>;
	private readonly stats: CacheStats = { hits: 0, misses: 0, evictions: 0 };
	private readonly listeners = new Array<EvictionListener<K, V>>();
	private clock = 0;

	public constructor(
		private readonly capacity: number,
		private readonly lifetime = math.huge,
	) {}

	public onEvict(listener: EvictionListener<K, V>): () => void {
		this.listeners.push(listener);
		return () => {
			const index = this.listeners.indexOf(listener);
			if (index >= 0) this.listeners.remove(index);
		};
	}

	public advance(time: number): void {
		this.clock += time;
	}

	public get(key: K): V | undefined {
		const entry = this.entries.get(key);
		if (!entry || entry.expiresAt <= this.clock) {
			if (entry) this.evict(entry);
			this.stats.misses++;
			return undefined;
		}
		this.stats.hits++;
		this.touch(entry);
		return entry.value;
	}

	public set(key: K, value: V): void {
		const existing = this.entries.get(key);
		if (existing) {
			existing.value = value;
			existing.expiresAt = this.clock + this.lifetime;
			this.touch(existing);
			return;
		}
		const entry: CacheEntry<K, V> = { key, value, expiresAt: this.clock + this.lifetime };
		this.entries.set(key, entry);
		this.attach(entry);
		while (this.entries.size() > this.capacity && this.oldest) this.evict(this.oldest);
	}

	public getOrCompute(key: K, compute: (key: K) => V): V {
		const cached = this.get(key);
		if (cached !== undefined) return cached;
		const value = compute(key);
		this.set(key, value);
		return value;
	}

	public size(): number {
		return this.entries.size();
	}

	public snapshot(): Readonly<CacheStats> {
		return { ...this.stats };
	}

	public hitRate(): number {
		const total = this.stats.hits + this.stats.misses;
		return total === 0 ? 0 : this.stats.hits / total;
	}

	public *keysNewestFirst(): Generator<K> {
		for (let entry = this.newest; entry; entry = entry.older) yield entry.key;
	}

	private touch(entry: CacheEntry<K, V>): void {
		if (entry === this.newest) return;
		this.detach(entry);
		this.attach(entry);
	}

	private attach(entry: CacheEntry<K, V>): void {
		entry.older = this.newest;
		entry.newer = undefined;
		if (this.newest) this.newest.newer = entry;
		this.newest = entry;
		if (!this.oldest) this.oldest = entry;
	}

	private detach(entry: CacheEntry<K, V>): void {
		if (entry.newer) entry.newer.older = entry.older;
		else this.newest = entry.older;
		if (entry.older) entry.older.newer = entry.newer;
		else this.oldest = entry.newer;
	}

	private evict(entry: CacheEntry<K, V>): void {
		this.detach(entry);
		this.entries.delete(entry.key);
		this.stats.evictions++;
		for (const listener of this.listeners) listener(entry.key, entry.value);
	}
}

export function memoizeWithCache<A extends defined, R extends defined>(capacity: number, compute: (argument: A) => R): (argument: A) => R {
	const cache = new LruCache<A, R>(capacity);
	return (argument) => cache.getOrCompute(argument, compute);
}
