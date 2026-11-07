import { AbstractCollection } from "./collection";

export class Deque<T extends defined> extends AbstractCollection<T> {
	private readonly items = new Map<number, T>();
	private front = 0;
	private back = 0;

	public constructor() {
		super();
	}

	public size(): number {
		return this.back - this.front;
	}

	public add(value: T): void {
		this.pushBack(value);
	}

	public clear(): void {
		this.items.clear();
		this.front = 0;
		this.back = 0;
	}

	public pushBack(value: T): void {
		this.items.set(this.back, value);
		this.back++;
	}

	public pushFront(value: T): void {
		this.front--;
		this.items.set(this.front, value);
	}

	public popBack(): T | undefined {
		if (this.isEmpty()) return undefined;
		this.back--;
		const value = this.items.get(this.back);
		this.items.delete(this.back);
		return value;
	}

	public popFront(): T | undefined {
		if (this.isEmpty()) return undefined;
		const value = this.items.get(this.front);
		this.items.delete(this.front);
		this.front++;
		return value;
	}

	public peekFront(): T | undefined {
		return this.items.get(this.front);
	}

	public peekBack(): T | undefined {
		return this.items.get(this.back - 1);
	}

	public at(index: number): T | undefined {
		return index >= 0 && index < this.size() ? this.items.get(this.front + index) : undefined;
	}

	public rotate(steps: number): void {
		if (this.isEmpty()) return;
		const amount = ((steps % this.size()) + this.size()) % this.size();
		for (let step = 0; step < amount; step++) this.pushFront(this.popBack()!);
	}

	public *values(): Generator<T> {
		for (let index = this.front; index < this.back; index++) yield this.items.get(index)!;
	}

	protected kind(): string {
		return "Deque";
	}
}

export class MonotonicQueue {
	private readonly deque = new Deque<[number, number]>();

	public push(index: number, value: number): void {
		while (!this.deque.isEmpty() && this.deque.peekBack()![1] <= value) this.deque.popBack();
		this.deque.pushBack([index, value]);
	}

	public expire(oldestIndex: number): void {
		while (!this.deque.isEmpty() && this.deque.peekFront()![0] < oldestIndex) this.deque.popFront();
	}

	public maximum(): number | undefined {
		return this.deque.peekFront()?.[1];
	}
}

export function slidingWindowMaximum(values: ReadonlyArray<number>, window: number): Array<number> {
	const queue = new MonotonicQueue();
	const result = new Array<number>();
	values.forEach((value, index) => {
		queue.push(index, value);
		queue.expire(index - window + 1);
		if (index >= window - 1) result.push(queue.maximum()!);
	});
	return result;
}

export function isPalindromeSequence<T extends defined>(values: ReadonlyArray<T>): boolean {
	const deque = new Deque<T>();
	deque.addAll(values);
	while (deque.size() > 1) {
		if (deque.popFront() !== deque.popBack()) return false;
	}
	return true;
}
