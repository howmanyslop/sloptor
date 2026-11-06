import { Deferred } from "./combinators";

export enum JobState {
	Pending,
	Running,
	Succeeded,
	Failed,
	Cancelled,
}

export interface JobOptions {
	readonly priority?: number;
	readonly label?: string;
}

interface Job<T> {
	readonly id: number;
	readonly label: string;
	readonly priority: number;
	readonly run: () => Promise<T>;
	readonly deferred: Deferred<T>;
	state: JobState;
}

export interface QueueStats {
	completed: number;
	failed: number;
	cancelled: number;
	peakRunning: number;
}

export class JobQueue {
	private readonly pending = new Array<Job<unknown>>();
	private readonly jobs = new Map<number, Job<unknown>>();
	private running = 0;
	private idCounter = 0;
	private paused = false;
	private readonly stats: QueueStats = { completed: 0, failed: 0, cancelled: 0, peakRunning: 0 };
	private readonly idleWaiters = new Array<Deferred<void>>();

	public constructor(private readonly concurrency: number) {}

	public enqueue<T>(run: () => Promise<T>, options: JobOptions = {}): [number, Promise<T>] {
		const id = ++this.idCounter;
		const deferred = new Deferred<T>();
		const job: Job<T> = {
			id,
			label: options.label ?? `job-${id}`,
			priority: options.priority ?? 0,
			run,
			deferred,
			state: JobState.Pending,
		};
		this.jobs.set(id, job as unknown as Job<unknown>);
		let insertAt = this.pending.size();
		while (insertAt > 0 && this.pending[insertAt - 1].priority < job.priority) insertAt--;
		this.pending.insert(insertAt, job as unknown as Job<unknown>);
		this.pump();
		return [id, deferred.promise];
	}

	public cancel(id: number): boolean {
		const job = this.jobs.get(id);
		if (!job || job.state !== JobState.Pending) return false;
		const index = this.pending.indexOf(job);
		if (index >= 0) this.pending.remove(index);
		job.state = JobState.Cancelled;
		job.deferred.reject(`cancelled ${job.label}`);
		this.stats.cancelled++;
		this.checkIdle();
		return true;
	}

	public pause(): void {
		this.paused = true;
	}

	public resume(): void {
		this.paused = false;
		this.pump();
	}

	public stateOf(id: number): JobState | undefined {
		return this.jobs.get(id)?.state;
	}

	public snapshot(): Readonly<QueueStats> {
		return { ...this.stats };
	}

	public onIdle(): Promise<void> {
		const waiter = new Deferred<void>();
		this.idleWaiters.push(waiter);
		this.checkIdle();
		return waiter.promise;
	}

	private pump(): void {
		while (!this.paused && this.running < this.concurrency && this.pending.size() > 0) {
			const job = this.pending.shift()!;
			this.execute(job);
		}
	}

	private execute(job: Job<unknown>): void {
		this.running++;
		this.stats.peakRunning = math.max(this.stats.peakRunning, this.running);
		job.state = JobState.Running;
		task.spawn(async () => {
			try {
				const value = await job.run();
				job.state = JobState.Succeeded;
				this.stats.completed++;
				job.deferred.resolve(value);
			} catch (reason) {
				job.state = JobState.Failed;
				this.stats.failed++;
				job.deferred.reject(reason);
			}
			this.running--;
			this.pump();
			this.checkIdle();
		});
	}

	private checkIdle(): void {
		if (this.running > 0 || this.pending.size() > 0) return;
		for (const waiter of this.idleWaiters) waiter.resolve(undefined);
		this.idleWaiters.clear();
	}
}
