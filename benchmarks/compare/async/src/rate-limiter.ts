import { sleep } from "./timing";

export interface Limiter {
	tryAcquire(cost?: number): boolean;
	acquire(cost?: number): Promise<void>;
}

export class TokenBucket implements Limiter {
	private tokens: number;
	private lastRefill = os.clock();

	public constructor(
		private readonly capacity: number,
		private readonly refillPerSecond: number,
	) {
		this.tokens = capacity;
	}

	private refill(): void {
		const now = os.clock();
		this.tokens = math.min(this.capacity, this.tokens + (now - this.lastRefill) * this.refillPerSecond);
		this.lastRefill = now;
	}

	public available(): number {
		this.refill();
		return this.tokens;
	}

	public tryAcquire(cost = 1): boolean {
		this.refill();
		if (this.tokens < cost) return false;
		this.tokens -= cost;
		return true;
	}

	public async acquire(cost = 1): Promise<void> {
		while (!this.tryAcquire(cost)) {
			const deficit = cost - this.tokens;
			await sleep(math.max(deficit / this.refillPerSecond, 1 / 60));
		}
	}
}

export class SlidingWindow implements Limiter {
	private readonly stamps = new Array<number>();

	public constructor(
		private readonly limit: number,
		private readonly windowSeconds: number,
	) {}

	private prune(now: number): void {
		while (this.stamps.size() > 0 && now - this.stamps[0] >= this.windowSeconds) this.stamps.shift();
	}

	public tryAcquire(cost = 1): boolean {
		const now = os.clock();
		this.prune(now);
		if (this.stamps.size() + cost > this.limit) return false;
		for (let index = 0; index < cost; index++) this.stamps.push(now);
		return true;
	}

	public async acquire(cost = 1): Promise<void> {
		while (!this.tryAcquire(cost)) {
			const oldest = this.stamps[0] ?? os.clock();
			await sleep(math.max(this.windowSeconds - (os.clock() - oldest), 1 / 60));
		}
	}
}

export class KeyedLimiter<K extends defined> {
	private readonly limiters = new Map<K, Limiter>();

	public constructor(private readonly factory: () => Limiter) {}

	public get(key: K): Limiter {
		let limiter = this.limiters.get(key);
		if (!limiter) {
			limiter = this.factory();
			this.limiters.set(key, limiter);
		}
		return limiter;
	}

	public tryAcquire(key: K, cost = 1): boolean {
		return this.get(key).tryAcquire(cost);
	}

	public forget(key: K): void {
		this.limiters.delete(key);
	}

	public size(): number {
		return this.limiters.size();
	}
}

export function limited<A extends Array<unknown>, R>(limiter: Limiter, callback: (...args: A) => Promise<R>): (...args: A) => Promise<R> {
	return async (...args: A) => {
		await limiter.acquire();
		return callback(...args);
	};
}
