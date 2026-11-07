import { sleep } from "./timing";

export type Settled<T> = { status: "fulfilled"; value: T } | { status: "rejected"; reason: unknown };

export async function allSettled<T>(promises: ReadonlyArray<Promise<T>>): Promise<Array<Settled<T>>> {
	const results = new Array<Settled<T>>();
	for (const promise of promises) {
		try {
			results.push({ status: "fulfilled", value: (await promise) as T });
		} catch (reason) {
			results.push({ status: "rejected", reason });
		}
	}
	return results;
}

export async function mapConcurrent<T extends defined, R extends defined>(
	items: ReadonlyArray<T>,
	limit: number,
	worker: (item: T, index: number) => Promise<R>,
): Promise<Array<R>> {
	const results = new Map<number, R>();
	let cursor = 0;
	const lane = async () => {
		while (cursor < items.size()) {
			const index = cursor++;
			results.set(index, (await worker(items[index], index)) as R);
		}
	};
	const lanes = new Array<Promise<void>>();
	for (let index = 0; index < math.min(limit, items.size()); index++) lanes.push(lane());
	await Promise.all(lanes);
	const ordered = new Array<R>();
	for (let index = 0; index < items.size(); index++) ordered.push(results.get(index)!);
	return ordered;
}

export async function sequence<T extends defined>(factories: ReadonlyArray<() => Promise<T>>): Promise<Array<T>> {
	const results = new Array<T>();
	for (const factory of factories) results.push((await factory()) as T);
	return results;
}

export async function firstSuccessful<T>(factories: ReadonlyArray<() => Promise<T>>): Promise<T | undefined> {
	for (const factory of factories) {
		const [ok, value] = await factory().then(
			(result): [boolean, T | undefined] => [true, result],
			(): [boolean, T | undefined] => [false, undefined],
		);
		if (ok) return value;
	}
	return undefined;
}

export function raceLabeled<T>(entries: ReadonlyArray<[string, Promise<T>]>): Promise<[string, T]> {
	return Promise.race(entries.map(([label, promise]) => promise.then((value): [string, T] => [label, value])));
}

export async function pollUntil(check: () => boolean, interval: number, limit: number): Promise<boolean> {
	const deadline = os.clock() + limit;
	while (os.clock() < deadline) {
		if (check()) return true;
		await sleep(interval);
	}
	return check();
}

export class Deferred<T> {
	public readonly promise: Promise<T>;
	private resolver?: (value: T) => void;
	private rejecter?: (reason: unknown) => void;
	private settled = false;

	public constructor() {
		this.promise = new Promise<T>((resolve, reject) => {
			this.resolver = resolve;
			this.rejecter = reject;
		});
	}

	public resolve(value: T): boolean {
		if (this.settled) return false;
		this.settled = true;
		this.resolver?.(value);
		return true;
	}

	public reject(reason: unknown): boolean {
		if (this.settled) return false;
		this.settled = true;
		this.rejecter?.(reason);
		return true;
	}

	public isSettled(): boolean {
		return this.settled;
	}
}

export class Latch {
	private remaining: number;
	private readonly done = new Deferred<void>();

	public constructor(count: number) {
		this.remaining = count;
		if (count <= 0) this.done.resolve(undefined);
	}

	public countDown(): void {
		this.remaining--;
		if (this.remaining <= 0) this.done.resolve(undefined);
	}

	public wait(): Promise<void> {
		return this.done.promise;
	}
}
