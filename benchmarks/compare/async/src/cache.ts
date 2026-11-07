interface Slot<V> {
	promise: Promise<V>;
	expiresAt: number;
}

export class AsyncCache<K extends defined, V> {
	private readonly slots = new Map<K, Slot<V>>();
	private hits = 0;
	private misses = 0;

	public constructor(
		private readonly loader: (key: K) => Promise<V>,
		private readonly lifetime: number,
	) {}

	public get(key: K): Promise<V> {
		const now = os.clock();
		const slot = this.slots.get(key);
		if (slot && slot.expiresAt > now) {
			this.hits++;
			return slot.promise;
		}
		this.misses++;
		const promise = this.loader(key);
		this.slots.set(key, { promise, expiresAt: now + this.lifetime });
		promise.catch(() => this.slots.delete(key));
		return promise;
	}

	public invalidate(key: K): void {
		this.slots.delete(key);
	}

	public ratio(): string {
		return `${this.hits}/${this.hits + this.misses}`;
	}
}

export class SingleFlight<K extends defined, V> {
	private readonly inFlight = new Map<K, Promise<V>>();

	public run(key: K, work: () => Promise<V>): Promise<V> {
		const existing = this.inFlight.get(key);
		if (existing) return existing;
		const promise = work().finally(() => {
			this.inFlight.delete(key);
		}) as Promise<V>;
		this.inFlight.set(key, promise);
		return promise;
	}

	public activeCount(): number {
		return this.inFlight.size();
	}
}

export class EventLog {
	private readonly lines = new Array<string>();
	private readonly started = os.clock();

	public record(message: string): void {
		this.lines.push(string.format("[%6.3f] %s", os.clock() - this.started, message));
	}

	public dump(): string {
		return this.lines.join("\n");
	}

	public size(): number {
		return this.lines.size();
	}
}
