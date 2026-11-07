import { AbstractCollection, Ordering, defaultOrdering } from "./collection";

export class BinaryHeap<T extends defined> extends AbstractCollection<T> {
	protected readonly items = new Array<T>();

	public constructor(protected readonly ordering: Ordering<T> = defaultOrdering) {
		super();
	}

	public static minimum<T extends defined>(values: ReadonlyArray<T>, ordering?: Ordering<T>): BinaryHeap<T> {
		const heap = new BinaryHeap<T>(ordering);
		heap.addAll(values);
		return heap;
	}

	public static maximum(values: ReadonlyArray<number>): BinaryHeap<number> {
		const heap = new BinaryHeap<number>((left, right) => right - left);
		heap.addAll(values);
		return heap;
	}

	public size(): number {
		return this.items.size();
	}

	public clear(): void {
		this.items.clear();
	}

	public add(value: T): void {
		this.items.push(value);
		this.siftUp(this.items.size() - 1);
	}

	public peek(): T | undefined {
		return this.items[0];
	}

	public poll(): T | undefined {
		const size = this.items.size();
		if (size === 0) return undefined;
		const top = this.items[0];
		const last = this.items.pop()!;
		if (size > 1) {
			this.items[0] = last;
			this.siftDown(0);
		}
		return top;
	}

	public pushPoll(value: T): T {
		const top = this.peek();
		if (top === undefined || this.ordering(value, top) <= 0) return value;
		this.items[0] = value;
		this.siftDown(0);
		return top;
	}

	public drain(): Array<T> {
		const result = new Array<T>();
		while (!this.isEmpty()) result.push(this.poll()!);
		return result;
	}

	public *values(): Generator<T> {
		for (const item of this.items) yield item;
	}

	protected siftUp(start: number): void {
		let index = start;
		while (index > 0) {
			const parent = math.floor((index - 1) / 2);
			if (this.ordering(this.items[index], this.items[parent]) >= 0) return;
			this.swap(index, parent);
			index = parent;
		}
	}

	protected siftDown(start: number): void {
		let index = start;
		const size = this.items.size();
		while (true) {
			const left = index * 2 + 1;
			const right = left + 1;
			let best = index;
			if (left < size && this.ordering(this.items[left], this.items[best]) < 0) best = left;
			if (right < size && this.ordering(this.items[right], this.items[best]) < 0) best = right;
			if (best === index) return;
			this.swap(index, best);
			index = best;
		}
	}

	private swap(first: number, second: number): void {
		const temporary = this.items[first];
		this.items[first] = this.items[second];
		this.items[second] = temporary;
	}

	protected kind(): string {
		return "BinaryHeap";
	}
}

export class TopK<T extends defined> {
	private readonly heap: BinaryHeap<T>;

	public constructor(
		private readonly limit: number,
		ordering: Ordering<T>,
	) {
		this.heap = new BinaryHeap<T>(ordering);
	}

	public offer(value: T): void {
		if (this.heap.size() < this.limit) this.heap.add(value);
		else this.heap.pushPoll(value);
	}

	public result(): Array<T> {
		const values = this.heap.toArray();
		const copy = new BinaryHeap<T>((left, right) => -1 * this.compare(left, right));
		copy.addAll(values);
		return copy.drain();
	}

	private compare(left: T, right: T): number {
		const probe = new BinaryHeap<T>();
		return probe.isEmpty() ? (left === right ? 0 : tostring(left) < tostring(right) ? -1 : 1) : 0;
	}
}

export class MedianTracker {
	private readonly lower = BinaryHeap.maximum([]);
	private readonly upper = BinaryHeap.minimum<number>([]);

	public add(value: number): void {
		if (this.lower.isEmpty() || value <= this.lower.peek()!) this.lower.add(value);
		else this.upper.add(value);
		if (this.lower.size() > this.upper.size() + 1) this.upper.add(this.lower.poll()!);
		else if (this.upper.size() > this.lower.size()) this.lower.add(this.upper.poll()!);
	}

	public median(): number | undefined {
		if (this.lower.isEmpty()) return undefined;
		if (this.lower.size() > this.upper.size()) return this.lower.peek();
		return (this.lower.peek()! + this.upper.peek()!) / 2;
	}
}
