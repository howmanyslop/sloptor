import { AbstractCollection, Equality } from "./collection";

class ListNode<T> {
	public forward?: ListNode<T>;
	public backward?: ListNode<T>;

	public constructor(public value: T) {}
}

export class LinkedList<T extends defined> extends AbstractCollection<T> {
	private head?: ListNode<T>;
	private tail?: ListNode<T>;
	private length = 0;

	public constructor(equality?: Equality<T>) {
		super(equality);
	}

	public static of<T extends defined>(...values: Array<T>): LinkedList<T> {
		const list = new LinkedList<T>();
		list.addAll(values);
		return list;
	}

	public size(): number {
		return this.length;
	}

	public add(value: T): void {
		this.pushBack(value);
	}

	public clear(): void {
		this.head = undefined;
		this.tail = undefined;
		this.length = 0;
	}

	public pushFront(value: T): void {
		const node = new ListNode(value);
		node.forward = this.head;
		if (this.head) this.head.backward = node;
		else this.tail = node;
		this.head = node;
		this.length++;
	}

	public pushBack(value: T): void {
		const node = new ListNode(value);
		node.backward = this.tail;
		if (this.tail) this.tail.forward = node;
		else this.head = node;
		this.tail = node;
		this.length++;
	}

	public popFront(): T | undefined {
		const node = this.head;
		if (!node) return undefined;
		this.unlink(node);
		return node.value;
	}

	public popBack(): T | undefined {
		const node = this.tail;
		if (!node) return undefined;
		this.unlink(node);
		return node.value;
	}

	public first(): T | undefined {
		return this.head?.value;
	}

	public last(): T | undefined {
		return this.tail?.value;
	}

	public at(index: number): T | undefined {
		if (index < 0 || index >= this.length) return undefined;
		let node = this.head;
		for (let step = 0; step < index && node; step++) node = node.forward;
		return node?.value;
	}

	public remove(value: T): boolean {
		for (let node = this.head; node; node = node.forward) {
			if (this.equality(node.value, value)) {
				this.unlink(node);
				return true;
			}
		}
		return false;
	}

	public removeWhere(predicate: (value: T) => boolean): number {
		let removed = 0;
		let node = this.head;
		while (node) {
			const following = node.forward;
			if (predicate(node.value)) {
				this.unlink(node);
				removed++;
			}
			node = following;
		}
		return removed;
	}

	public reverse(): void {
		let node = this.head;
		this.tail = node;
		let previous: ListNode<T> | undefined;
		while (node) {
			const following = node.forward;
			node.forward = previous;
			node.backward = following;
			previous = node;
			node = following;
		}
		this.head = previous;
	}

	public map<U extends defined>(transform: (value: T) => U): LinkedList<U> {
		const result = new LinkedList<U>();
		for (const value of this.values()) result.pushBack(transform(value));
		return result;
	}

	public filter(predicate: (value: T) => boolean): LinkedList<T> {
		const result = new LinkedList<T>(this.equality);
		for (const value of this.values()) if (predicate(value)) result.pushBack(value);
		return result;
	}

	public *values(): Generator<T> {
		for (let node = this.head; node; node = node.forward) yield node.value;
	}

	public *reversed(): Generator<T> {
		for (let node = this.tail; node; node = node.backward) yield node.value;
	}

	private unlink(node: ListNode<T>): void {
		if (node.backward) node.backward.forward = node.forward;
		else this.head = node.forward;
		if (node.forward) node.forward.backward = node.backward;
		else this.tail = node.backward;
		node.forward = undefined;
		node.backward = undefined;
		this.length--;
	}

	protected kind(): string {
		return "LinkedList";
	}
}
