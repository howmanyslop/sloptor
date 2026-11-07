export function sleep(seconds: number): Promise<number> {
	return new Promise((resolve) => {
		task.delay(seconds, () => resolve(seconds));
	});
}

export function nextFrame(): Promise<number> {
	return new Promise((resolve) => resolve(task.wait()));
}

export class TimeoutError {
	public constructor(
		public readonly label: string,
		public readonly seconds: number,
	) {}

	public describe(): string {
		return `${this.label} timed out after ${this.seconds}s`;
	}
}

export async function withTimeout<T>(promise: Promise<T>, seconds: number, label = "operation"): Promise<T> {
	const timer = Promise.delay(seconds).then((): T => {
		throw new TimeoutError(label, seconds);
	});
	try {
		return (await Promise.race([promise, timer])) as T;
	} finally {
		timer.cancel();
	}
}

export async function retry<T>(attempt: (index: number) => Promise<T>, attempts: number, backoff = 0.1): Promise<T> {
	let lastError: unknown;
	for (let index = 0; index < attempts; index++) {
		try {
			return (await attempt(index)) as T;
		} catch (error) {
			lastError = error;
			if (index < attempts - 1) await sleep(backoff * 2 ** index);
		}
	}
	throw lastError;
}

export function retryWithLibrary<T>(callback: () => Promise<T>, attempts: number): Promise<T> {
	return Promise.retry(callback, attempts);
}

export function debounce<A extends Array<unknown>>(seconds: number, callback: (...args: A) => void): (...args: A) => void {
	let generation = 0;
	return (...args: A) => {
		generation++;
		const mine = generation;
		task.delay(seconds, () => {
			if (mine === generation) callback(...args);
		});
	};
}

export function throttle<A extends Array<unknown>>(seconds: number, callback: (...args: A) => void): (...args: A) => void {
	let readyAt = 0;
	return (...args: A) => {
		const now = os.clock();
		if (now < readyAt) return;
		readyAt = now + seconds;
		callback(...args);
	};
}

export class Stopwatch {
	private started = os.clock();
	private laps = new Array<number>();

	public lap(): number {
		const now = os.clock();
		const previous = this.laps.size() > 0 ? this.laps[this.laps.size() - 1] : this.started;
		this.laps.push(now);
		return now - previous;
	}

	public elapsed(): number {
		return os.clock() - this.started;
	}

	public reset(): void {
		this.started = os.clock();
		this.laps = [];
	}
}
