export interface ScheduledTask {
	readonly id: number;
	readonly name: string;
	cancel(): void;
}

interface Entry {
	id: number;
	name: string;
	interval: number;
	runAt: number;
	repeating: boolean;
	callback: () => void;
	cancelled: boolean;
	runs: number;
}

export class Scheduler {
	private readonly entries = new Map<number, Entry>();
	private counter = 0;
	private thread?: thread;
	private clock = 0;

	public every(name: string, interval: number, callback: () => void): ScheduledTask {
		return this.schedule(name, interval, true, callback);
	}

	public after(name: string, delay: number, callback: () => void): ScheduledTask {
		return this.schedule(name, delay, false, callback);
	}

	private schedule(name: string, interval: number, repeating: boolean, callback: () => void): ScheduledTask {
		const id = ++this.counter;
		const entry: Entry = { id, name, interval, runAt: this.clock + interval, repeating, callback, cancelled: false, runs: 0 };
		this.entries.set(id, entry);
		const entries = this.entries;
		return {
			id,
			name,
			cancel() {
				entry.cancelled = true;
				entries.delete(id);
			},
		};
	}

	public tick(dt: number): number {
		this.clock += dt;
		const due = new Array<Entry>();
		for (const [, entry] of this.entries) if (!entry.cancelled && entry.runAt <= this.clock) due.push(entry);
		due.sort((left, right) => (left.runAt === right.runAt ? left.id < right.id : left.runAt < right.runAt));
		for (const entry of due) {
			entry.runs++;
			const [ok, problem] = pcall(entry.callback);
			if (!ok) warn(`scheduled task ${entry.name} failed: ${tostring(problem)}`);
			if (entry.repeating && !entry.cancelled) entry.runAt += entry.interval;
			else this.entries.delete(entry.id);
		}
		return due.size();
	}

	public start(): void {
		if (this.thread) return;
		this.thread = task.spawn(() => {
			while (true) this.tick(task.wait());
		});
	}

	public stop(): void {
		if (this.thread) task.cancel(this.thread);
		this.thread = undefined;
	}

	public pendingCount(): number {
		return this.entries.size();
	}

	public runsOf(id: number): number {
		return this.entries.get(id)?.runs ?? 0;
	}
}

export function deferBatch(callbacks: ReadonlyArray<() => void>): void {
	for (const callback of callbacks) task.defer(callback);
}

export function staggered(count: number, gap: number, callback: (index: number) => void): void {
	for (let index = 0; index < count; index++) task.delay(index * gap, callback, index);
}

export function cooperativeLoop(items: ReadonlyArray<number>, budgetPerFrame: number, process: (item: number) => void): thread {
	return task.spawn(() => {
		let used = 0;
		for (const item of items) {
			process(item);
			used++;
			if (used >= budgetPerFrame) {
				used = 0;
				task.wait();
			}
		}
	});
}
