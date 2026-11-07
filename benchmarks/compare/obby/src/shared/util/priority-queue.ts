interface HeapEntry<T> {
	readonly value: T;
	readonly priority: number;
	readonly order: number;
}

/** Binary min-heap. Ties are broken by insertion order for determinism. */
export class PriorityQueue<T extends defined> {
	private readonly heap = new Array<HeapEntry<T>>();
	private insertions = 0;

	public static from<T extends defined>(items: ReadonlyArray<T>, score: (item: T) => number): PriorityQueue<T> {
		const queue = new PriorityQueue<T>();
		for (const item of items) {
			queue.push(item, score(item));
		}
		return queue;
	}

	public size(): number {
		return this.heap.size();
	}

	public isEmpty(): boolean {
		return this.heap.isEmpty();
	}

	public push(value: T, priority: number): void {
		this.heap.push({ value, priority, order: this.insertions++ });
		this.siftUp(this.heap.size() - 1);
	}

	public peek(): T | undefined {
		return this.heap[0]?.value;
	}

	public peekPriority(): number | undefined {
		return this.heap[0]?.priority;
	}

	public pop(): T | undefined {
		const count = this.heap.size();
		if (count === 0) {
			return undefined;
		}
		const top = this.heap[0];
		const last = this.heap.pop()!;
		if (count > 1) {
			this.heap[0] = last;
			this.siftDown(0);
		}
		return top.value;
	}

	public drain(): Array<T> {
		const result = new Array<T>();
		let item = this.pop();
		while (item !== undefined) {
			result.push(item);
			item = this.pop();
		}
		return result;
	}

	public clear(): void {
		this.heap.clear();
	}

	private less(a: HeapEntry<T>, b: HeapEntry<T>): boolean {
		return a.priority < b.priority || (a.priority === b.priority && a.order < b.order);
	}

	private siftUp(start: number): void {
		let child = start;
		while (child > 0) {
			const parent = math.floor((child - 1) / 2);
			if (!this.less(this.heap[child], this.heap[parent])) {
				break;
			}
			[this.heap[child], this.heap[parent]] = [this.heap[parent], this.heap[child]];
			child = parent;
		}
	}

	private siftDown(start: number): void {
		const count = this.heap.size();
		let parent = start;
		while (true) {
			const left = parent * 2 + 1;
			const right = left + 1;
			let smallest = parent;
			if (left < count && this.less(this.heap[left], this.heap[smallest])) {
				smallest = left;
			}
			if (right < count && this.less(this.heap[right], this.heap[smallest])) {
				smallest = right;
			}
			if (smallest === parent) {
				return;
			}
			[this.heap[parent], this.heap[smallest]] = [this.heap[smallest], this.heap[parent]];
			parent = smallest;
		}
	}
}
