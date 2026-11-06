import { AbstractCollection } from "./collection";

export enum OverflowPolicy {
	Reject,
	Overwrite,
}

export class RingBuffer<T extends defined> extends AbstractCollection<T> {
	private readonly slots = new Map<number, T>();
	private start = 0;
	private length = 0;
	private dropped = 0;

	public constructor(
		private readonly capacity: number,
		private readonly policy = OverflowPolicy.Overwrite,
	) {
		super();
		assert(capacity > 0, "capacity must be positive");
	}

	public size(): number {
		return this.length;
	}

	public getCapacity(): number {
		return this.capacity;
	}

	public droppedCount(): number {
		return this.dropped;
	}

	public isFull(): boolean {
		return this.length === this.capacity;
	}

	public add(value: T): void {
		this.push(value);
	}

	public push(value: T): boolean {
		if (this.isFull()) {
			if (this.policy === OverflowPolicy.Reject) return false;
			this.slots.set(this.start, value);
			this.start = (this.start + 1) % this.capacity;
			this.dropped++;
			return true;
		}
		this.slots.set((this.start + this.length) % this.capacity, value);
		this.length++;
		return true;
	}

	public shift(): T | undefined {
		if (this.length === 0) return undefined;
		const value = this.slots.get(this.start);
		this.slots.delete(this.start);
		this.start = (this.start + 1) % this.capacity;
		this.length--;
		return value;
	}

	public peek(offset = 0): T | undefined {
		if (offset < 0 || offset >= this.length) return undefined;
		return this.slots.get((this.start + offset) % this.capacity);
	}

	public newest(): T | undefined {
		return this.peek(this.length - 1);
	}

	public clear(): void {
		this.slots.clear();
		this.start = 0;
		this.length = 0;
	}

	public *values(): Generator<T> {
		for (let offset = 0; offset < this.length; offset++) yield this.slots.get((this.start + offset) % this.capacity)!;
	}

	protected kind(): string {
		return "RingBuffer";
	}
}

export class RollingAverage {
	private readonly buffer: RingBuffer<number>;
	private sum = 0;

	public constructor(window: number) {
		this.buffer = new RingBuffer<number>(window, OverflowPolicy.Reject);
	}

	public sample(value: number): number {
		if (this.buffer.isFull()) this.sum -= this.buffer.shift()!;
		this.buffer.push(value);
		this.sum += value;
		return this.average();
	}

	public average(): number {
		return this.buffer.isEmpty() ? 0 : this.sum / this.buffer.size();
	}

	public minimum(): number {
		return this.buffer.reduce(math.huge, (low, value) => math.min(low, value));
	}

	public maximum(): number {
		return this.buffer.reduce(-math.huge, (high, value) => math.max(high, value));
	}
}
