export interface ReadonlyCollection<T extends defined> {
	size(): number;
	isEmpty(): boolean;
	contains(value: T): boolean;
	values(): Generator<T>;
	toArray(): Array<T>;
}

export interface Collection<T extends defined> extends ReadonlyCollection<T> {
	add(value: T): void;
	clear(): void;
}

export type Equality<T> = (left: T, right: T) => boolean;
export type Ordering<T> = (left: T, right: T) => number;

export function defaultEquality<T>(left: T, right: T): boolean {
	return left === right;
}

export function defaultOrdering<T>(left: T, right: T): number {
	const a = left as unknown as number;
	const b = right as unknown as number;
	return a < b ? -1 : a > b ? 1 : 0;
}

export abstract class AbstractCollection<T extends defined> implements Collection<T> {
	protected static instances = 0;
	protected readonly equality: Equality<T>;

	protected constructor(equality: Equality<T> = defaultEquality) {
		this.equality = equality;
		AbstractCollection.instances++;
	}

	public static liveCount(): number {
		return AbstractCollection.instances;
	}

	public abstract size(): number;
	public abstract add(value: T): void;
	public abstract clear(): void;
	public abstract values(): Generator<T>;

	public isEmpty(): boolean {
		return this.size() === 0;
	}

	public contains(value: T): boolean {
		for (const item of this.values()) {
			if (this.equality(item, value)) return true;
		}
		return false;
	}

	public toArray(): Array<T> {
		const result = new Array<T>();
		for (const item of this.values()) result.push(item);
		return result;
	}

	public addAll(values: ReadonlyArray<T>): this {
		for (const value of values) this.add(value);
		return this;
	}

	public reduce<A>(initial: A, step: (accumulator: A, value: T) => A): A {
		let accumulator = initial;
		for (const item of this.values()) accumulator = step(accumulator, item);
		return accumulator;
	}

	public count(predicate: (value: T) => boolean): number {
		return this.reduce(0, (total, value) => (predicate(value) ? total + 1 : total));
	}

	public describe(): string {
		const parts = new Array<string>();
		for (const item of this.values()) parts.push(tostring(item));
		return `${this.kind()}[${parts.join(", ")}]`;
	}

	protected abstract kind(): string;
}
