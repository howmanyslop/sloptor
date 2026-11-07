import { Deferred } from "./combinators";

export class Channel<T extends defined> {
	private readonly buffer = new Array<T>();
	private readonly receivers = new Array<Deferred<T | undefined>>();
	private readonly senders = new Array<[T, Deferred<boolean>]>();
	private closed = false;

	public constructor(private readonly capacity = 0) {}

	public send(value: T): Promise<boolean> {
		if (this.closed) return Promise.resolve(false);
		const receiver = this.receivers.shift();
		if (receiver) {
			receiver.resolve(value);
			return Promise.resolve(true);
		}
		if (this.buffer.size() < this.capacity) {
			this.buffer.push(value);
			return Promise.resolve(true);
		}
		const deferred = new Deferred<boolean>();
		this.senders.push([value, deferred]);
		return deferred.promise;
	}

	public receive(): Promise<T | undefined> {
		const buffered = this.buffer.shift();
		if (buffered !== undefined) {
			const waiting = this.senders.shift();
			if (waiting) {
				this.buffer.push(waiting[0]);
				waiting[1].resolve(true);
			}
			return Promise.resolve(buffered);
		}
		const waiting = this.senders.shift();
		if (waiting) {
			waiting[1].resolve(true);
			return Promise.resolve(waiting[0]);
		}
		if (this.closed) return Promise.resolve(undefined);
		const deferred = new Deferred<T | undefined>();
		this.receivers.push(deferred);
		return deferred.promise;
	}

	public close(): void {
		this.closed = true;
		for (const receiver of this.receivers) receiver.resolve(undefined);
		for (const [, sender] of this.senders) sender.resolve(false);
		this.receivers.clear();
		this.senders.clear();
	}

	public isClosed(): boolean {
		return this.closed;
	}

	public async drain(): Promise<Array<T>> {
		const items = new Array<T>();
		while (true) {
			const value = await this.receive();
			if (value === undefined) return items;
			items.push(value as T);
		}
	}
}

export async function pipeline<A extends defined, B extends defined>(
	input: Channel<A>,
	output: Channel<B>,
	transform: (value: A) => Promise<B>,
): Promise<number> {
	let processed = 0;
	while (true) {
		const value = await input.receive();
		if (value === undefined) break;
		await output.send((await transform(value as A)) as B);
		processed++;
	}
	output.close();
	return processed;
}

export class Mutex {
	private locked = false;
	private readonly waiters = new Array<Deferred<void>>();

	public async lock(): Promise<() => void> {
		if (this.locked) {
			const waiter = new Deferred<void>();
			this.waiters.push(waiter);
			await waiter.promise;
		}
		this.locked = true;
		let released = false;
		return () => {
			if (released) return;
			released = true;
			this.locked = false;
			this.waiters.shift()?.resolve(undefined);
		};
	}

	public async run<T>(callback: () => Promise<T>): Promise<T> {
		const release = await this.lock();
		try {
			return (await callback()) as T;
		} finally {
			release();
		}
	}
}
